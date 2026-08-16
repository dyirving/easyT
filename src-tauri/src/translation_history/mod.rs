pub mod models;
mod worker;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, oneshot};

pub use models::{
    ClearHistoryResult, HistoryCommitEligibility, HistoryCommitOutcome, HistoryEntryDraft,
    HistoryLimitResult, HistoryLimitUpdate, HistorySnapshot, HistoryWarning, RequestEligibility,
    SaveConfigResult, TranslationHistoryEntry,
};
use worker::HistoryDatabase;

const COMMAND_QUEUE_CAPACITY: usize = 64;
const SHUTDOWN_BUDGET: Duration = Duration::from_secs(1);

#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("翻译历史不可用")]
    Unavailable,
    #[error("翻译历史数据库损坏")]
    CorruptDatabase,
    #[error("不支持的翻译历史数据库版本")]
    UnsupportedSchema,
    #[error("翻译历史记录损坏")]
    CorruptEntry,
    #[error("翻译历史记录过大")]
    EntryTooLarge,
    #[error("翻译历史记录不存在")]
    NotFound,
    #[error("翻译历史记录 ID 无效")]
    InvalidEntryId,
    #[error("翻译历史上限无效")]
    InvalidLimit,
    #[error("历史保存已取消")]
    Cancelled,
}

enum HistoryCommand {
    #[cfg(test)]
    BlockForTest {
        duration: Duration,
        started: oneshot::Sender<()>,
    },
    Initialize {
        reply: oneshot::Sender<Result<HistorySnapshot, HistoryError>>,
    },
    Get {
        entry_id: String,
        reply: oneshot::Sender<Result<TranslationHistoryEntry, HistoryError>>,
    },
    Commit {
        draft: Box<HistoryEntryDraft>,
        limit: u8,
        eligibility: HistoryCommitEligibility,
        reply: oneshot::Sender<Result<HistoryCommitOutcome, HistoryError>>,
    },
    Clear {
        reply: oneshot::Sender<Result<ClearHistoryResult, HistoryError>>,
    },
    ApplyLimit {
        limit: u8,
        reply: oneshot::Sender<Result<HistoryLimitResult, HistoryError>>,
    },
    Shutdown {
        reply: oneshot::Sender<()>,
    },
}

pub struct TranslationHistory {
    sender: mpsc::Sender<HistoryCommand>,
}

impl TranslationHistory {
    pub fn start(data_dir: &Path, initial_limit: u8) -> Arc<Self> {
        let (sender, mut receiver) = mpsc::channel(COMMAND_QUEUE_CAPACITY);
        let data_dir = data_dir.to_path_buf();
        let spawn = std::thread::Builder::new()
            .name("easyT-history-db".to_string())
            .stack_size(512 * 1024)
            .spawn(move || {
                let mut database = HistoryDatabase::open(&data_dir, initial_limit).ok();
                while let Some(command) = receiver.blocking_recv() {
                    match command {
                        #[cfg(test)]
                        HistoryCommand::BlockForTest { duration, started } => {
                            let _ = started.send(());
                            std::thread::sleep(duration);
                        }
                        HistoryCommand::Initialize { reply } => {
                            if database.is_none() {
                                database = HistoryDatabase::open(&data_dir, initial_limit).ok();
                            }
                            let result = database
                                .as_mut()
                                .ok_or(HistoryError::Unavailable)
                                .and_then(HistoryDatabase::snapshot);
                            let _ = reply.send(result);
                        }
                        HistoryCommand::Get { entry_id, reply } => {
                            let result = database
                                .as_ref()
                                .ok_or(HistoryError::Unavailable)
                                .and_then(|db| db.get_entry(&entry_id));
                            let _ = reply.send(result);
                        }
                        HistoryCommand::Commit {
                            draft,
                            limit,
                            eligibility,
                            reply,
                        } => {
                            if reply.is_closed() {
                                eligibility.cancel();
                            }
                            let result = database
                                .as_mut()
                                .ok_or(HistoryError::Unavailable)
                                .and_then(|db| {
                                    db.commit_entry(*draft, limit, &eligibility)
                                });
                            let _ = reply.send(result);
                        }
                        HistoryCommand::Clear { reply } => {
                            let result = database
                                .as_mut()
                                .ok_or(HistoryError::Unavailable)
                                .and_then(HistoryDatabase::clear_all);
                            let _ = reply.send(result);
                        }
                        HistoryCommand::ApplyLimit { limit, reply } => {
                            let result = database
                                .as_mut()
                                .ok_or(HistoryError::Unavailable)
                                .and_then(|db| db.apply_limit(limit));
                            let _ = reply.send(result);
                        }
                        HistoryCommand::Shutdown { reply } => {
                            drop(database.take());
                            let _ = reply.send(());
                            break;
                        }
                    }
                }
            });
        if spawn.is_err() {
            log::warn!(
                "history_worker_state_changed: from=starting to=unavailable reason=thread_spawn"
            );
        }
        Arc::new(Self { sender })
    }

    pub async fn initialize(&self) -> Result<HistorySnapshot, HistoryError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(HistoryCommand::Initialize { reply })
            .await
            .map_err(|_| HistoryError::Unavailable)?;
        receiver.await.map_err(|_| HistoryError::Unavailable)?
    }

    pub async fn get_entry(
        &self,
        entry_id: String,
    ) -> Result<TranslationHistoryEntry, HistoryError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(HistoryCommand::Get { entry_id, reply })
            .await
            .map_err(|_| HistoryError::Unavailable)?;
        receiver.await.map_err(|_| HistoryError::Unavailable)?
    }

    pub async fn commit_entry(
        &self,
        draft: HistoryEntryDraft,
        limit: u8,
        eligibility: HistoryCommitEligibility,
    ) -> HistoryCommitOutcome {
        let (reply, mut receiver) = oneshot::channel();
        let token = eligibility.clone();
        let command = HistoryCommand::Commit {
            draft: Box::new(draft),
            limit,
            eligibility,
            reply,
        };
        match tokio::time::timeout(token.remaining(), self.sender.send(command)).await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => {
                token.cancel();
                return HistoryCommitOutcome::NotSaved {
                    warning: HistoryWarning::save_failed(),
                };
            }
            Err(_) => {
                token.cancel();
                return HistoryCommitOutcome::NotSaved {
                    warning: HistoryWarning::timed_out(),
                };
            }
        }
        match tokio::time::timeout(token.remaining(), &mut receiver).await {
            Ok(result) => map_commit_reply(result),
            Err(_) if token.cancel() => HistoryCommitOutcome::NotSaved {
                warning: HistoryWarning::timed_out(),
            },
            Err(_) if token.commit_claimed() => {
                // Worker 已在期限内取得 COMMIT 所有权；此时必须等待其真实结果，
                // 避免数据库已提交却向前端谎报未保存。
                match receiver.await {
                    Ok(result) => map_commit_reply(Ok(result)),
                    Err(_) => HistoryCommitOutcome::NotSaved {
                        warning: HistoryWarning::save_failed(),
                    },
                }
            }
            Err(_) => HistoryCommitOutcome::NotSaved {
                warning: HistoryWarning::save_failed(),
            },
        }
    }

    pub async fn clear_all(&self) -> Result<ClearHistoryResult, HistoryError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(HistoryCommand::Clear { reply })
            .await
            .map_err(|_| HistoryError::Unavailable)?;
        receiver.await.map_err(|_| HistoryError::Unavailable)?
    }

    pub async fn apply_limit(&self, limit: u8) -> Result<HistoryLimitResult, HistoryError> {
        let (reply, receiver) = oneshot::channel();
        self.sender
            .send(HistoryCommand::ApplyLimit { limit, reply })
            .await
            .map_err(|_| HistoryError::Unavailable)?;
        receiver.await.map_err(|_| HistoryError::Unavailable)?
    }

    pub async fn shutdown(&self) {
        let result = tokio::time::timeout(SHUTDOWN_BUDGET, async {
            let (reply, receiver) = oneshot::channel();
            self.sender
                .send(HistoryCommand::Shutdown { reply })
                .await
                .map_err(|_| ())?;
            receiver.await.map_err(|_| ())
        })
        .await;
        if !matches!(result, Ok(Ok(()))) {
            log::warn!("history_shutdown_timeout: state=stopping");
        }
    }

    #[cfg(test)]
    async fn block_worker_for_test(&self, duration: Duration) {
        let (started, receiver) = oneshot::channel();
        self.sender
            .send(HistoryCommand::BlockForTest { duration, started })
            .await
            .expect("history worker should accept test block");
        receiver
            .await
            .expect("history worker should begin test block");
    }
}

fn map_commit_reply(
    result: Result<Result<HistoryCommitOutcome, HistoryError>, oneshot::error::RecvError>,
) -> HistoryCommitOutcome {
    match result {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(HistoryError::EntryTooLarge)) => HistoryCommitOutcome::NotSaved {
            warning: HistoryWarning::too_large(),
        },
        Ok(Err(HistoryError::Cancelled)) => HistoryCommitOutcome::NotSaved {
            warning: HistoryWarning::save_failed(),
        },
        Ok(Err(_)) | Err(_) => HistoryCommitOutcome::NotSaved {
            warning: HistoryWarning::save_failed(),
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::{atomic::AtomicBool, Arc};
    use std::time::Instant;

    use crate::translation_backend::models::{BackendMode, BackendSource};

    use super::*;

    #[tokio::test]
    async fn queued_commit_times_out_and_never_reaches_the_database() {
        let data_dir =
            std::env::temp_dir().join(format!("easyt-history-timeout-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&data_dir).expect("temp dir");
        let history = TranslationHistory::start(&data_dir, 5);
        history.initialize().await.expect("initialize");
        history
            .block_worker_for_test(Duration::from_millis(2_100))
            .await;

        let started_at = Instant::now();
        let outcome = history
            .commit_entry(
                HistoryEntryDraft::new(
                    "source".to_string(),
                    "target".to_string(),
                    "简体中文".to_string(),
                    BackendSource {
                        backend: BackendMode::OfficialApi,
                        provider: "agnes".to_string(),
                        model: "agnes-2.0-flash".to_string(),
                    },
                    false,
                    started_at,
                ),
                5,
                HistoryCommitEligibility::new(RequestEligibility::new(Arc::new(AtomicBool::new(
                    true,
                )))),
            )
            .await;
        assert!(matches!(
            outcome,
            HistoryCommitOutcome::NotSaved {
                warning: HistoryWarning {
                    kind: models::HistoryWarningKind::SaveTimedOut,
                    ..
                }
            }
        ));
        assert!(started_at.elapsed() < Duration::from_millis(2_300));

        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(history
            .initialize()
            .await
            .expect("snapshot")
            .summaries
            .is_empty());
        history.shutdown().await;
        drop(history);
        let _ = fs::remove_dir_all(data_dir);
    }
}
