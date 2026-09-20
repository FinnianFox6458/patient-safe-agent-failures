use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use patient_safe_agent_failures::{
    appointment_guard::{assess_failure, AppointmentFailure, WorkflowAction},
    infrai_errors::{CaptureError, InfraiErrors},
};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Serialize)]
struct DecisionResponse {
    appointment_ref: String,
    decision: WorkflowAction,
    failure_recorded: bool,
}

#[derive(Debug, Error)]
enum ServiceError {
    #[error("{0}")]
    Capture(#[from] CaptureError),
}

impl IntoResponse for ServiceError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::Capture(CaptureError::Rejected { status, .. }) if status.is_client_error() => *status,
            _ => StatusCode::BAD_GATEWAY,
        };
        (status, Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

async fn record_failure(
    State(infrai): State<InfraiErrors>,
    Json(input): Json<AppointmentFailure>,
) -> Result<Json<DecisionResponse>, ServiceError> {
    let decision = assess_failure(&input);
    infrai.capture(&decision.capture).await?;
    Ok(Json(DecisionResponse {
        appointment_ref: input.appointment_ref,
        decision: decision.action,
        failure_recorded: true,
    }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let infrai = InfraiErrors::from_env()?;
    let app = Router::new()
        .route("/appointment-failures", post(record_failure))
        .with_state(infrai);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await?;
    println!("appointment failure service listening on http://127.0.0.1:3000");
    axum::serve(listener, app).await?;
    Ok(())
}

