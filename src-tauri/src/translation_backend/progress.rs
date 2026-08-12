use std::{
    sync::{Arc, Mutex},
    time::Instant,
};

use serde::{Deserialize, Serialize};

use super::{error::BackendError, models::BackendMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TranslationPhase {
    CheckingCache,
    PreparingRequest,
    ConnectingBackend,
    WaitingForContent,
    ReceivingContent,
    SavingHistory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressBackendSource {
    pub mode: BackendMode,
    pub provider: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseProgress {
    pub sequence: u64,
    pub phase: TranslationPhase,
    pub total_elapsed_ms: u64,
    pub backend: Option<ProgressBackendSource>,
}

pub trait TranslationProgress: Send + Sync {
    fn phase_changed(&self, progress: PhaseProgress);
    fn content_delta(&self, delta: String) -> Result<(), BackendError>;
}

pub struct TranslationProgressReporter {
    started_at: Instant,
    sink: Option<Arc<dyn TranslationProgress>>,
    state: Mutex<ReporterState>,
}

#[derive(Default)]
struct ReporterState {
    sequence: u64,
    phase: Option<TranslationPhase>,
    backend: Option<ProgressBackendSource>,
}

impl TranslationProgressReporter {
    pub fn new(sink: Arc<dyn TranslationProgress>) -> Self {
        Self {
            started_at: Instant::now(),
            sink: Some(sink),
            state: Mutex::new(ReporterState::default()),
        }
    }

    pub fn discard() -> Self {
        Self {
            started_at: Instant::now(),
            sink: None,
            state: Mutex::new(ReporterState::default()),
        }
    }

    pub fn phase(&self, phase: TranslationPhase, backend: Option<ProgressBackendSource>) {
        let Some(sink) = self.sink.as_ref() else {
            return;
        };
        let progress = {
            let mut state = match self.state.lock() {
                Ok(state) => state,
                Err(poisoned) => poisoned.into_inner(),
            };
            if !is_valid_transition(state.phase, phase) {
                log::warn!(
                    "translation_progress_transition_ignored: from={:?} to={:?}",
                    state.phase,
                    phase
                );
                return;
            }
            state.sequence = state.sequence.saturating_add(1);
            state.phase = Some(phase);
            state.backend = if phase == TranslationPhase::CheckingCache {
                None
            } else {
                backend.or_else(|| state.backend.clone())
            };
            PhaseProgress {
                sequence: state.sequence,
                phase,
                total_elapsed_ms: self.elapsed_ms(),
                backend: state.backend.clone(),
            }
        };
        sink.phase_changed(progress);
    }

    pub fn content_delta(&self, delta: String) -> Result<(), BackendError> {
        match self.sink.as_ref() {
            Some(sink) => sink.content_delta(delta),
            None => Ok(()),
        }
    }

    pub fn elapsed_ms(&self) -> u64 {
        self.started_at
            .elapsed()
            .as_millis()
            .min(u128::from(u64::MAX)) as u64
    }
}

fn is_valid_transition(current: Option<TranslationPhase>, next: TranslationPhase) -> bool {
    matches!(
        (current, next),
        (
            None,
            TranslationPhase::CheckingCache | TranslationPhase::PreparingRequest
        ) | (
            Some(TranslationPhase::CheckingCache),
            TranslationPhase::PreparingRequest
        ) | (
            Some(TranslationPhase::PreparingRequest),
            TranslationPhase::ConnectingBackend
        ) | (
            Some(TranslationPhase::ConnectingBackend),
            TranslationPhase::ConnectingBackend | TranslationPhase::WaitingForContent
        ) | (
            Some(TranslationPhase::WaitingForContent),
            TranslationPhase::ReceivingContent | TranslationPhase::SavingHistory
        ) | (
            Some(TranslationPhase::ReceivingContent),
            TranslationPhase::SavingHistory
        ) | (
            Some(TranslationPhase::CheckingCache),
            TranslationPhase::SavingHistory
        )
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct RecordingSink {
        phases: Mutex<Vec<PhaseProgress>>,
    }

    impl TranslationProgress for RecordingSink {
        fn phase_changed(&self, progress: PhaseProgress) {
            self.phases.lock().expect("phase lock").push(progress);
        }

        fn content_delta(&self, _delta: String) -> Result<(), BackendError> {
            Ok(())
        }
    }

    #[test]
    fn reports_the_real_phase_sequence_from_one() {
        let sink = Arc::new(RecordingSink::default());
        let reporter = TranslationProgressReporter::new(sink.clone());

        reporter.phase(TranslationPhase::CheckingCache, None);
        reporter.phase(
            TranslationPhase::PreparingRequest,
            Some(ProgressBackendSource {
                mode: BackendMode::OfficialApi,
                provider: "deepseek".to_string(),
            }),
        );

        let phases = sink.phases.lock().expect("phase lock");
        assert_eq!(phases.len(), 2);
        assert_eq!(phases[0].sequence, 1);
        assert_eq!(phases[0].phase, TranslationPhase::CheckingCache);
        assert_eq!(phases[1].sequence, 2);
        assert_eq!(phases[1].phase, TranslationPhase::PreparingRequest);
    }

    #[test]
    fn only_a_real_connection_retry_repeats_a_phase() {
        let sink = Arc::new(RecordingSink::default());
        let reporter = TranslationProgressReporter::new(sink.clone());

        reporter.phase(TranslationPhase::PreparingRequest, None);
        reporter.phase(TranslationPhase::PreparingRequest, None);
        reporter.phase(TranslationPhase::ConnectingBackend, None);
        reporter.phase(TranslationPhase::ConnectingBackend, None);
        reporter.phase(TranslationPhase::WaitingForContent, None);
        reporter.phase(TranslationPhase::PreparingRequest, None);

        let phases = sink.phases.lock().expect("phase lock");
        assert_eq!(
            phases.iter().map(|event| event.phase).collect::<Vec<_>>(),
            vec![
                TranslationPhase::PreparingRequest,
                TranslationPhase::ConnectingBackend,
                TranslationPhase::ConnectingBackend,
                TranslationPhase::WaitingForContent,
            ]
        );
        assert_eq!(
            phases
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
    }

    #[test]
    fn saving_history_is_a_real_terminal_phase_for_cache_and_network_success() {
        for phases in [
            vec![
                TranslationPhase::CheckingCache,
                TranslationPhase::SavingHistory,
            ],
            vec![
                TranslationPhase::PreparingRequest,
                TranslationPhase::ConnectingBackend,
                TranslationPhase::WaitingForContent,
                TranslationPhase::ReceivingContent,
                TranslationPhase::SavingHistory,
            ],
        ] {
            let sink = Arc::new(RecordingSink::default());
            let reporter = TranslationProgressReporter::new(sink.clone());
            for phase in &phases {
                reporter.phase(*phase, None);
            }
            assert_eq!(
                sink.phases
                    .lock()
                    .expect("phase lock")
                    .iter()
                    .map(|event| event.phase)
                    .collect::<Vec<_>>(),
                phases
            );
        }
    }
}
