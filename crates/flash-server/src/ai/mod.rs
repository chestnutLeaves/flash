//! Card generation and voice grading through a language model. The
//! server owns the pipeline; the model is reached through one trait so
//! the hosted product, a self-hoster's own key and the test harness
//! all run the same code with a different provider behind it.
//!
//! `provider` is the contract (inputs, outputs, errors, and what the
//! configured model can be handed); `openai` and `gemini` are the two
//! adapters, one per API, and `build_provider` is the one place a
//! configuration becomes one of them; `fake` is the scripted stand-in
//! every test uses; `prompts` and `schema` are the words and the reply
//! shapes sent to the model, `replies` reads the typed reply back out of
//! the JSON, and `transport` is what the adapters share of the wire;
//! `validate` turns what the model returned into fields the note editor
//! would have accepted; `sources` decides what an upload is; `convert`
//! runs the operator's document converter; `runner` is the one loop
//! that takes queued jobs through the model, and `ledger` writes every
//! call, answered or not, with what the provider counted, and hands it
//! to the policy.
//!
//! The kind of a source is one vocabulary everywhere: the store's
//! `AiSourceKind` is the column, the policy's `AiInput` and the
//! provider's `SourceKind` are the same type under the names those
//! modules speak.

use std::sync::Arc;

pub mod caps;
pub mod convert;
pub mod fake;
pub mod gemini;
pub mod ledger;
pub mod openai;
#[cfg(feature = "pdf")]
pub mod pdf;
pub mod prompts;
pub mod provider;
pub mod replies;
pub mod runner;
pub mod schema;
pub mod sources;
mod transport;
pub mod validate;

/// The adapter a configuration names, built. The only place a provider
/// is named outside its own module: the boot code asks here and
/// mentions none.
pub fn build_provider(
    config: &crate::config::AiConfig,
) -> Result<Arc<dyn provider::LlmProvider>, String> {
    Ok(match config.provider {
        crate::config::AiProviderKind::Openai => {
            Arc::new(openai::OpenAiProvider::from_config(config)?)
        }
        crate::config::AiProviderKind::Gemini => {
            Arc::new(gemini::GeminiProvider::from_config(config))
        }
    })
}
