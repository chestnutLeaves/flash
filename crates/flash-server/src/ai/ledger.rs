//! One ledger row per model call, answered or not, written the moment
//! the reply is in hand. The generation runner and the voice study
//! path both come through here, so every call is recorded the same
//! way: the provider's own counts, the exchange around them, and the
//! class of failure (never the provider's body) when there was one. A
//! reply the provider billed and then declined or garbled still
//! carries its usage, and is recorded with it. The policy hears of
//! every row as it is written and may keep its own account of the
//! call; the open server keeps none.

use std::time::Instant;

use flash_core::{AiJobId, UserId};
use flash_store::{AiCallNew, AiCallStep, AiSourceKind, Store};

use super::provider::{LlmError, Reply, Usage};
use crate::policy::{AiPolicy, RecordedCall};

/// What the ledger needs to know about a call besides its reply.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    pub job: Option<AiJobId>,
    pub step: AiCallStep,
    /// The model as configured.
    pub model: &'a str,
    pub kind: AiSourceKind,
    pub started: Instant,
    pub started_at: i64,
}

/// Records the call, tells the policy, and returns the row id. What the
/// policy makes of the call is its own affair: an error there is logged
/// and the call stands recorded.
pub fn record<T>(
    store: &Store,
    policy: &dyn AiPolicy,
    user: UserId,
    call: &Call<'_>,
    reply: &Result<Reply<T>, LlmError>,
) -> flash_store::Result<i64> {
    let elapsed_ms = call.started.elapsed().as_millis().min(u128::from(u32::MAX)) as u32;
    let nothing = Usage::default();
    let (usage, error): (&Usage, Option<String>) = match reply {
        Ok(reply) => (&reply.usage, None),
        Err(e) => (e.usage().unwrap_or(&nothing), Some(error_class(e))),
    };
    let id = store.insert_ai_call(
        user,
        &AiCallNew {
            job_id: call.job,
            step: call.step,
            model: call.model,
            model_version: usage.model_version.as_deref(),
            response_id: usage.response_id.as_deref(),
            input_kind: call.kind,
            input_tokens: usage.input_tokens(),
            cached_tokens: usage.cached_tokens(),
            tool_tokens: usage.tool_tokens(),
            output_tokens: usage.output_tokens(),
            thoughts_tokens: usage.thoughts,
            media_seconds: usage.media_seconds.unwrap_or(0),
            ms: if usage.ms > 0 { usage.ms } else { elapsed_ms },
            usage_json: usage.raw.as_deref(),
            ok: error.is_none(),
            error: error.as_deref(),
            started_at: call.started_at,
        },
    )?;
    let recorded = RecordedCall {
        id,
        user,
        model: call.model,
        step: call.step,
        usage,
        ok: error.is_none(),
        started_at: call.started_at,
    };
    if let Err(e) = policy.call_recorded(store, &recorded) {
        tracing::warn!(call = id, "the policy could not account for the call: {e}");
    }
    Ok(id)
}

/// The class of a failure, for the ledger's error column: never the
/// provider's body.
pub fn error_class(e: &LlmError) -> String {
    match e {
        LlmError::Http { status, .. } => format!("http {status}"),
        LlmError::Refused { .. } => "refused".to_string(),
        LlmError::Schema { .. } => "schema".to_string(),
        LlmError::Timeout => "timeout".to_string(),
        LlmError::Config(_) => "config".to_string(),
    }
}
