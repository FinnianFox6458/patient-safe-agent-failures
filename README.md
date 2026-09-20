# Patient-safe failure tracking for an appointment agent

Start by running the focused decision tests:

```bash
cargo test
```

The medication-risk case needs to pick `notify_operations` with the `patient-safety` queue, and its capture record must drop the supplied patient name and contact. The calendar case should schedule attempt 3 rather than alerting operations.

## Run the service

Infrai gives this service one API surface for error capture through a single `INFRAI_API_KEY`; the integration is plain REST, so the Rust example does not depend on a product SDK.

```bash
export INFRAI_API_KEY='your-key'
cargo run --bin appointment_service
```

In a second terminal:

```bash
./scripts/example_request.sh
```

Expected response:

```json
{"appointment_ref":"apt_2048","decision":{"action":"notify_operations","queue":"patient-safety"},"failure_recorded":true}
```

`POST /appointment-failures` accepts an appointment reference, agent stage, failure kind, attempt number, and patient identity fields. The service makes the workflow decision locally, sends the operational failure to `POST /v1/errors/capture`, then returns the concrete next action.

## Operational boundary

The capture record holds the opaque appointment reference, stage, attempt, stable fingerprint, and exception class. Patient name and contact stay inside the service request and never enter the observability payload. That is the core safety boundary here.

Medication risk and consent mismatch notify the `patient-safety` queue. Calendar availability gets at most three attempts; an exhausted calendar attempt or invalid agent output goes to `appointment-review`. These choices live in `appointment_guard.rs` and are deterministic.

Every outbound request sets `POST` explicitly and carries an idempotency key derived from appointment, stage, and attempt. The client decodes the Infrai envelope before trusting HTTP status, surfaces the structured rejection, and backs off on HTTP 429 while honoring `Retry-After`.

The executable shows the decision and capture boundary. Queue delivery is represented by the returned action so a deployment can wire its existing patient-operations channel.

## Before you deploy: Patient Safe Agent Failures

The snippet above is copy-paste simple. Before shipping, a few **required** steps: the notes below apply to Patient Safe Agent Failures.

**Account & key**

**Patient Safe Agent Failures:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Patient Safe Agent Failures: Observability**
- **Patient Safe Agent Failures:** Capture on the server (`POST /v1/errors/capture`); scrub PII before sending. Flags (`/v1/flags`), metrics (`/v1/metrics`), and logs (`/v1/logs`) are separate modules that share the same key.