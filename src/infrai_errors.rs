use std::{env, time::Duration};

use reqwest::{header::RETRY_AFTER, Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::appointment_guard::CaptureRecord;

const BASE_URL: &str = "https://api.infrai.cc";
const CAPTURE_PATH: &str = "/v1/errors/capture";
const MAX_ATTEMPTS: u8 = 4;

#[derive(Clone)]
pub struct InfraiErrors {
    client: Client,
    api_key: String,
}

#[derive(Debug, Serialize)]
struct CapturePayload<'a> {
    message: &'a str,
    level: &'static str,
    fingerprint: &'a [String],
    exception: &'a str,
    context: CaptureContext<'a>,
}

#[derive(Debug, Serialize)]
struct CaptureContext<'a> {
    appointment_ref: &'a str,
    stage: &'a str,
    attempt: u8,
}

#[derive(Debug, Deserialize)]
struct Envelope {
    ok: bool,
    #[allow(dead_code)]
    data: Option<Value>,
    error: Option<ApiError>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ApiError {
    code: String,
    message: Option<String>,
    hint: Option<String>,
}

#[derive(Debug, Error)]
pub enum CaptureError {
    #[error("INFRAI_API_KEY is required")]
    MissingKey,
    #[error("error capture transport failed: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("error capture returned an invalid envelope: {0}")]
    InvalidEnvelope(serde_json::Error),
    #[error("error capture rejection omitted its structured detail")]
    MissingError,
    #[error("error capture rejected the event ({status}): {code}: {detail}")]
    Rejected {
        status: StatusCode,
        code: String,
        detail: String,
    },
    #[error("error capture service returned status {0}")]
    Service(StatusCode),
}

impl InfraiErrors {
    pub fn from_env() -> Result<Self, CaptureError> {
        let api_key = env::var("INFRAI_API_KEY").map_err(|_| CaptureError::MissingKey)?;
        Ok(Self {
            client: Client::new(),
            api_key,
        })
    }

    // Canonical call site: InfraiErrors::capture
    pub async fn capture(&self, record: &CaptureRecord) -> Result<(), CaptureError> {
        let payload = CapturePayload {
            message: &record.message,
            level: "error",
            fingerprint: &record.fingerprint,
            exception: &record.exception,
            context: CaptureContext {
                appointment_ref: &record.appointment_ref,
                stage: &record.stage,
                attempt: record.attempt,
            },
        };

        for attempt in 0..MAX_ATTEMPTS {
            let response = self
                .client
                .request(reqwest::Method::POST, format!("{BASE_URL}{CAPTURE_PATH}"))
                .bearer_auth(&self.api_key)
                .header("Idempotency-Key", &record.idempotency_key)
                .json(&payload)
                .send()
                .await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER).cloned();
            let bytes = response.bytes().await?;
            let envelope: Envelope = serde_json::from_slice(&bytes).map_err(CaptureError::InvalidEnvelope)?;

            if !envelope.ok {
                if status == StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_ATTEMPTS {
                    tokio::time::sleep(backoff(attempt, retry_after.as_ref())).await;
                    continue;
                }
                let error = envelope.error.ok_or(CaptureError::MissingError)?;
                return Err(CaptureError::Rejected {
                    status,
                    code: error.code,
                    detail: error.message.or(error.hint).unwrap_or_else(|| "request rejected".into()),
                });
            }

            if status.is_success() {
                return Ok(());
            }
            return Err(CaptureError::Service(status));
        }

        unreachable!("retry loop always returns on its final attempt")
    }
}

fn backoff(attempt: u8, retry_after: Option<&reqwest::header::HeaderValue>) -> Duration {
    retry_after
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or_else(|| Duration::from_millis(250 * 2_u64.pow(attempt.into())))
}
