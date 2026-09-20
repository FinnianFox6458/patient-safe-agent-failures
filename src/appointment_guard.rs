use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct AppointmentFailure {
    pub appointment_ref: String,
    pub stage: AgentStage,
    pub kind: FailureKind,
    pub attempt: u8,
    pub patient_name: String,
    pub patient_contact: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStage {
    Eligibility,
    SlotSelection,
    Confirmation,
}

impl AgentStage {
    fn label(self) -> &'static str {
        match self {
            Self::Eligibility => "eligibility",
            Self::SlotSelection => "slot_selection",
            Self::Confirmation => "confirmation",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    CalendarUnavailable,
    ConsentMismatch,
    MedicationRisk,
    InvalidAgentOutput,
}

impl FailureKind {
    fn label(self) -> &'static str {
        match self {
            Self::CalendarUnavailable => "calendar_unavailable",
            Self::ConsentMismatch => "consent_mismatch",
            Self::MedicationRisk => "medication_risk",
            Self::InvalidAgentOutput => "invalid_agent_output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum WorkflowAction {
    RetryQueued { next_attempt: u8 },
    NotifyOperations { queue: &'static str },
    ManualReview { queue: &'static str },
}

#[derive(Debug, Clone)]
pub struct CaptureRecord {
    pub message: String,
    pub fingerprint: Vec<String>,
    pub exception: String,
    pub appointment_ref: String,
    pub stage: String,
    pub attempt: u8,
    pub idempotency_key: String,
}

pub struct FailureDecision {
    pub action: WorkflowAction,
    pub capture: CaptureRecord,
}

pub fn assess_failure(input: &AppointmentFailure) -> FailureDecision {
    let action = match input.kind {
        FailureKind::ConsentMismatch | FailureKind::MedicationRisk => {
            WorkflowAction::NotifyOperations {
                queue: "patient-safety",
            }
        }
        FailureKind::CalendarUnavailable if input.attempt < 3 => WorkflowAction::RetryQueued {
            next_attempt: input.attempt + 1,
        },
        FailureKind::CalendarUnavailable | FailureKind::InvalidAgentOutput => {
            WorkflowAction::ManualReview {
                queue: "appointment-review",
            }
        }
    };

    let stage = input.stage.label().to_owned();
    let kind = input.kind.label();
    FailureDecision {
        action,
        capture: CaptureRecord {
            message: format!("appointment agent stopped at {stage}: {kind}"),
            fingerprint: vec!["appointment-agent".into(), stage.clone(), kind.into()],
            exception: format!("AgentWorkflowError: {kind}"),
            appointment_ref: input.appointment_ref.clone(),
            stage,
            attempt: input.attempt,
            idempotency_key: format!(
                "appointment:{}:{}:{}",
                input.appointment_ref,
                input.stage.label(),
                input.attempt
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medication_risk_notifies_ops_without_patient_identifiers_in_capture() {
        let failure = AppointmentFailure {
            appointment_ref: "apt_2048".into(),
            stage: AgentStage::Confirmation,
            kind: FailureKind::MedicationRisk,
            attempt: 1,
            patient_name: "Private Name".into(),
            patient_contact: "private@example.test".into(),
        };

        let decision = assess_failure(&failure);

        assert_eq!(
            decision.action,
            WorkflowAction::NotifyOperations {
                queue: "patient-safety"
            }
        );
        let capture = format!("{:?}", decision.capture);
        assert!(!capture.contains(&failure.patient_name));
        assert!(!capture.contains(&failure.patient_contact));
    }

    #[test]
    fn temporary_calendar_failure_queues_a_bounded_retry() {
        let failure = AppointmentFailure {
            appointment_ref: "apt_2049".into(),
            stage: AgentStage::SlotSelection,
            kind: FailureKind::CalendarUnavailable,
            attempt: 2,
            patient_name: String::new(),
            patient_contact: String::new(),
        };

        assert_eq!(
            assess_failure(&failure).action,
            WorkflowAction::RetryQueued { next_attempt: 3 }
        );
    }
}

