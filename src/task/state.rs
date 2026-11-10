//! Typed pipeline status and execution phase.
//!
//! Both enums render `SCREAMING_SNAKE_CASE` JSON exactly like the free-form strings they
//! replaced, so existing `pipeline-*.json` archives and CLI output stay identical. A status
//! this build does not know (hand edit, archive from a newer build) still loads through
//! [`TaskStatus::Unrecognized`] instead of failing closed at parse time, preserving the
//! permissive behavior of the untyped field.
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Loaded,
    Running,
    Completed,
    Paused,
    Interrupted,
    Reconciled,
    Cancelled,
    PersistenceError,
    Unrecognized(String),
}

impl TaskStatus {
    pub(crate) fn parse(value: &str) -> Self {
        match value {
            "LOADED" => Self::Loaded,
            "RUNNING" => Self::Running,
            "COMPLETED" => Self::Completed,
            "PAUSED" => Self::Paused,
            "INTERRUPTED" => Self::Interrupted,
            "RECONCILED" => Self::Reconciled,
            "CANCELLED" => Self::Cancelled,
            "PERSISTENCE_ERROR" => Self::PersistenceError,
            other => Self::Unrecognized(other.to_owned()),
        }
    }
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::Loaded => "LOADED",
            Self::Running => "RUNNING",
            Self::Completed => "COMPLETED",
            Self::Paused => "PAUSED",
            Self::Interrupted => "INTERRUPTED",
            Self::Reconciled => "RECONCILED",
            Self::Cancelled => "CANCELLED",
            Self::PersistenceError => "PERSISTENCE_ERROR",
            Self::Unrecognized(raw) => raw,
        }
    }
    /// Terminal archive: loading may validate topology only, since archived actions are
    /// data rather than executable CLI input.
    pub(crate) fn is_archived(&self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
    /// No admitted work and no operator action pending; inspection may report quiet.
    pub(crate) fn is_quiet(&self) -> bool {
        matches!(self, Self::Loaded | Self::Completed | Self::Cancelled)
    }
    /// Operator attention required; inspection sets `needsReview`.
    pub(crate) fn needs_review(&self) -> bool {
        matches!(
            self,
            Self::Paused | Self::Interrupted | Self::PersistenceError | Self::Unrecognized(_)
        )
    }
    /// Finished or stalled states publish a notification event.
    pub(crate) fn publishes_notification(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Cancelled | Self::Paused | Self::Reconciled | Self::Interrupted
        )
    }
    /// Only these may continue through `resume`; cancelled tasks stay cancelled.
    pub(crate) fn is_resumable(&self) -> bool {
        matches!(self, Self::Paused | Self::Interrupted | Self::Reconciled)
    }
}

impl Serialize for TaskStatus {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for TaskStatus {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Preparing,
    StartingListener,
    Waiting,
    Executing,
}

impl Phase {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "PREPARING" => Some(Self::Preparing),
            "STARTING_LISTENER" => Some(Self::StartingListener),
            "WAITING" => Some(Self::Waiting),
            "EXECUTING" => Some(Self::Executing),
            _ => None,
        }
    }
    pub(crate) fn as_str(&self) -> &'static str {
        match self {
            Self::Preparing => "PREPARING",
            Self::StartingListener => "STARTING_LISTENER",
            Self::Waiting => "WAITING",
            Self::Executing => "EXECUTING",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const STATUSES: [(TaskStatus, &str); 8] = [
        (TaskStatus::Loaded, "LOADED"),
        (TaskStatus::Running, "RUNNING"),
        (TaskStatus::Completed, "COMPLETED"),
        (TaskStatus::Paused, "PAUSED"),
        (TaskStatus::Interrupted, "INTERRUPTED"),
        (TaskStatus::Reconciled, "RECONCILED"),
        (TaskStatus::Cancelled, "CANCELLED"),
        (TaskStatus::PersistenceError, "PERSISTENCE_ERROR"),
    ];

    #[test]
    fn status_renders_the_exact_legacy_strings() {
        for (status, text) in STATUSES {
            assert_eq!(serde_json::to_value(&status).unwrap(), json!(text));
            assert_eq!(
                serde_json::from_value::<TaskStatus>(json!(text)).unwrap(),
                status
            );
            assert_eq!(TaskStatus::parse(text), status);
            assert_eq!(status.as_str(), text);
        }
    }

    #[test]
    fn unknown_status_strings_stay_loadable_and_byte_identical() {
        let unknown: TaskStatus = serde_json::from_value(json!("FUTURE_STATE")).unwrap();
        assert_eq!(unknown, TaskStatus::Unrecognized("FUTURE_STATE".into()));
        assert_eq!(
            serde_json::to_value(&unknown).unwrap(),
            json!("FUTURE_STATE")
        );
        assert!(!unknown.is_archived() && !unknown.is_quiet());
        assert!(unknown.needs_review() && !unknown.publishes_notification());
        assert!(!unknown.is_resumable());
        let non_string: Result<TaskStatus, _> = serde_json::from_value(serde_json::Value::Null);
        assert!(non_string.is_err());
    }

    #[test]
    fn status_classification_sets_match_the_previous_string_matches() {
        let quiet = STATUSES
            .iter()
            .filter(|(status, _)| status.is_quiet())
            .map(|(_, text)| *text)
            .collect::<Vec<_>>();
        assert_eq!(quiet, ["LOADED", "COMPLETED", "CANCELLED"]);
        let archived = STATUSES
            .iter()
            .filter(|(status, _)| status.is_archived())
            .map(|(_, text)| *text)
            .collect::<Vec<_>>();
        assert_eq!(archived, ["COMPLETED", "CANCELLED"]);
        let resumable = STATUSES
            .iter()
            .filter(|(status, _)| status.is_resumable())
            .map(|(_, text)| *text)
            .collect::<Vec<_>>();
        assert_eq!(resumable, ["PAUSED", "INTERRUPTED", "RECONCILED"]);
        let notified = STATUSES
            .iter()
            .filter(|(status, _)| status.publishes_notification())
            .map(|(_, text)| *text)
            .collect::<Vec<_>>();
        assert_eq!(
            notified,
            [
                "COMPLETED",
                "PAUSED",
                "INTERRUPTED",
                "RECONCILED",
                "CANCELLED"
            ]
        );
        let review = STATUSES
            .iter()
            .filter(|(status, _)| status.needs_review())
            .map(|(_, text)| *text)
            .collect::<Vec<_>>();
        assert_eq!(review, ["PAUSED", "INTERRUPTED", "PERSISTENCE_ERROR"]);
    }

    #[test]
    fn phase_renders_the_exact_legacy_strings_and_rejects_unknown_values() {
        for text in ["PREPARING", "STARTING_LISTENER", "WAITING", "EXECUTING"] {
            let phase = Phase::parse(text).unwrap();
            assert_eq!(phase.as_str(), text);
            assert_eq!(serde_json::to_value(phase.as_str()).unwrap(), json!(text));
        }
        for text in ["", "PAUSED", "preparing", "EXECUTING "] {
            assert!(Phase::parse(text).is_none(), "{text}");
        }
    }
}
