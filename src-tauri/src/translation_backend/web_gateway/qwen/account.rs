use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::QwenError;

pub const MAXIMUM_ACCOUNTS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct AccountId(String);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }

    pub fn parse(value: &str) -> Result<Self, QwenError> {
        let parsed = Uuid::parse_str(value).map_err(|_| QwenError::invalid_account_id())?;
        if parsed.get_version_num() != 4 || parsed.to_string() != value {
            return Err(QwenError::invalid_account_id());
        }
        Ok(Self(value.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for AccountId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayName(String);

impl DisplayName {
    pub fn parse(value: &str) -> Result<Self, QwenError> {
        if value.chars().any(char::is_control) {
            return Err(QwenError::invalid_display_name());
        }
        let trimmed = value.trim();
        let length = trimmed.chars().count();
        if !(1..=40).contains(&length) {
            return Err(QwenError::invalid_display_name());
        }
        Ok(Self(trimmed.to_string()))
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PersistedLogin {
    LoggedOut,
    Ready,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PersistedHealth {
    Unknown,
    Healthy,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAccount {
    pub account_id: AccountId,
    pub display_name: String,
    pub enabled: bool,
    pub login_state: PersistedLogin,
    pub last_health: PersistedHealth,
}

impl PersistedAccount {
    #[cfg(test)]
    pub fn new(display_name: DisplayName) -> Self {
        Self {
            account_id: AccountId::new(),
            display_name: display_name.0,
            enabled: true,
            login_state: PersistedLogin::LoggedOut,
            last_health: PersistedHealth::Unknown,
        }
    }

    pub fn validate(&self) -> Result<(), QwenError> {
        AccountId::parse(self.account_id.as_str())?;
        DisplayName::parse(&self.display_name)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum QwenAccountDisplayStatus {
    Disabled,
    LoggingIn,
    LoggedOut,
    Expired,
    Busy,
    CoolingDown,
    PendingVerification,
    Available,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountMoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwenAccountSnapshot {
    pub account_id: AccountId,
    pub display_name: String,
    pub enabled: bool,
    pub order: usize,
    pub status: QwenAccountDisplayStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cooldown_remaining_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_code: Option<String>,
    pub actions: QwenAccountActions,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwenAccountActions {
    pub can_rename: bool,
    pub can_toggle_enabled: bool,
    pub can_move_up: bool,
    pub can_move_down: bool,
    pub can_login: bool,
    pub can_logout: bool,
    pub can_test: bool,
    pub can_delete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwenAccountPoolSnapshot {
    pub accounts: Vec<QwenAccountSnapshot>,
    pub maximum_accounts: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_account_id: Option<AccountId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<QwenAccountPoolWarning>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QwenAccountPoolWarning {
    pub code: String,
    pub message: String,
}

impl QwenAccountPoolSnapshot {
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            accounts: Vec::new(),
            maximum_accounts: MAXIMUM_ACCOUNTS,
            login_account_id: None,
            warning: None,
        }
    }
}

pub fn display_status(account: &PersistedAccount) -> QwenAccountDisplayStatus {
    if !account.enabled {
        QwenAccountDisplayStatus::Disabled
    } else if account.login_state == PersistedLogin::LoggedOut {
        QwenAccountDisplayStatus::LoggedOut
    } else if account.login_state == PersistedLogin::Expired {
        QwenAccountDisplayStatus::Expired
    } else if account.last_health == PersistedHealth::Unhealthy {
        QwenAccountDisplayStatus::PendingVerification
    } else {
        QwenAccountDisplayStatus::Available
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_id_requires_a_canonical_v4_uuid() {
        let id = "550e8400-e29b-41d4-a716-446655440000";
        assert!(AccountId::parse(id).is_ok());
        assert!(AccountId::parse("550E8400-E29B-41D4-A716-446655440000").is_err());
        assert!(AccountId::parse("550e8400-e29b-11d4-a716-446655440000").is_err());
    }

    #[test]
    fn display_name_trims_counts_unicode_and_rejects_controls() {
        assert_eq!(
            DisplayName::parse("  default  ").unwrap().as_str(),
            "default"
        );
        assert!(DisplayName::parse("").is_err());
        assert!(DisplayName::parse("\nname").is_err());
        assert!(DisplayName::parse(&"a".repeat(41)).is_err());
        assert!(DisplayName::parse(&"账".repeat(40)).is_ok());
    }

    #[test]
    fn pool_snapshot_does_not_serialize_sensitive_storage_fields() {
        let snapshot = QwenAccountPoolSnapshot::empty();
        let json = serde_json::to_string(&snapshot).unwrap();
        assert!(!json.contains("credentials"));
        assert!(!json.contains("ticket"));
        assert!(!json.contains("cookie"));
    }
}
