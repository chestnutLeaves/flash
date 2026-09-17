//! The one place a plan can say no: whether `adding` cards may be added
//! and how large today's new-card budget may be. `Services` consults a
//! `CardPolicy` and never looks at plan state itself, so the open server
//! runs unlimited and a hosted build plugs in its cap.

use flash_core::UserId;
use flash_store::{Store, StoreError};

/// Where an add originates. A policy may treat a one-off import more
/// generously than an interactive add (web form, MCP tool).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddSource {
    Interactive,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Go ahead. `recheck_cap` is the live-card cap the store re-checks
    /// inside its own insert transaction (None = unlimited).
    Allowed { recheck_cap: Option<u32> },
    /// Refused: the account already holds `current` of `cap` cards.
    Denied { current: u32, cap: u32 },
}

pub trait CardPolicy: Send + Sync + 'static {
    /// Decides an add of `adding` cards. `adding == 0` never denies (an
    /// edit that inserts nothing must always go through). It only reports
    /// the cap to re-check.
    fn admit(
        &self,
        store: &Store,
        user: UserId,
        adding: u32,
        source: AddSource,
        now_ms: i64,
    ) -> Result<Admission, StoreError>;

    /// Clamps today's account-wide new-card budget.
    fn new_budget(
        &self,
        store: &Store,
        user: UserId,
        budget: u32,
        now_ms: i64,
    ) -> Result<u32, StoreError>;

    /// The sentence a refused add carries. It reaches AI assistants
    /// verbatim over MCP, so it must state the limit plainly.
    fn over_cap_message(&self, current: u32, cap: u32) -> String;
}

/// No cap, no clamp: the open server's policy.
pub struct Unlimited;

impl CardPolicy for Unlimited {
    fn admit(
        &self,
        _store: &Store,
        _user: UserId,
        _adding: u32,
        _source: AddSource,
        _now_ms: i64,
    ) -> Result<Admission, StoreError> {
        Ok(Admission::Allowed { recheck_cap: None })
    }

    fn new_budget(
        &self,
        _store: &Store,
        _user: UserId,
        budget: u32,
        _now_ms: i64,
    ) -> Result<u32, StoreError> {
        Ok(budget)
    }

    fn over_cap_message(&self, current: u32, cap: u32) -> String {
        format!("card limit reached: {current} of {cap} cards")
    }
}

/// What a generation is fed. The policy sees the kinds, not the bytes:
/// a plan may sell video and audio separately from text. The store's
/// own kind is the one vocabulary the whole pipeline speaks.
pub type AiInput = flash_store::AiSourceKind;

/// One generation about to be admitted: every kind in the set it reads
/// (deduplicated) and, for media, the playing time summed over the
/// members whose length is known before the model is called. A set is
/// admitted only when every kind in it is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiRequest {
    pub inputs: Vec<AiInput>,
    pub media_seconds: Option<u32>,
}

impl AiRequest {
    /// A request over one kind.
    pub fn single(input: AiInput, media_seconds: Option<u32>) -> Self {
        AiRequest {
            inputs: vec![input],
            media_seconds,
        }
    }

    pub fn has_video(&self) -> bool {
        self.inputs.iter().any(|i| i.is_video())
    }

    pub fn has_audio(&self) -> bool {
        self.inputs.iter().any(|i| i.is_audio())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAdmission {
    Allowed,
    /// Refused. `code` is a stable token clients branch on
    /// (`pro_required`, `monthly_limit`, `hourly_limit`, `media_limit`),
    /// from a generation or from a spoken turn alike; `message` is the
    /// sentence a person reads.
    Denied {
        code: &'static str,
        message: String,
    },
}

/// The account's standing this month, for the meter a client shows
/// before a submit. Every field is None when nothing is metered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AiUsage {
    /// How much of the month's allowance is used, 0 to 100, whatever
    /// the policy meters it in; None when nothing is metered.
    pub allowance_used_pct: Option<u8>,
    /// When the month starts over, for "resets Oct 1".
    pub resets_at_ms: Option<i64>,
    /// Inputs this account may submit, regardless of what the server
    /// has enabled; the intersection is what the UI offers.
    pub inputs: &'static [AiInput],
}

/// A model call the ledger just wrote, handed to the policy so it can
/// keep its own account of it.
#[derive(Debug, Clone, Copy)]
pub struct RecordedCall<'a> {
    /// The ledger row.
    pub id: i64,
    pub user: UserId,
    /// The model as configured.
    pub model: &'a str,
    pub step: flash_store::AiCallStep,
    /// What the provider counted, whether or not it answered.
    pub usage: &'a crate::ai::provider::Usage,
    pub ok: bool,
    pub started_at: i64,
}

/// The one place a plan can say no to a model call. `Unlimited` on the
/// open server; a hosted build plugs in its monthly and hourly caps.
/// Usage is read from the call ledger the server writes; a policy may
/// keep its own account of each call beside it, and the open server
/// keeps none.
pub trait AiPolicy: Send + Sync + 'static {
    fn admit_generation(
        &self,
        store: &Store,
        user: UserId,
        request: AiRequest,
        now_ms: i64,
    ) -> Result<AiAdmission, StoreError>;

    /// A spoken study turn about to go to the model. Allowed unless a
    /// policy says otherwise.
    fn admit_voice(
        &self,
        _store: &Store,
        _user: UserId,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        Ok(AiAdmission::Allowed)
    }

    fn usage(&self, store: &Store, user: UserId, now_ms: i64) -> Result<AiUsage, StoreError>;

    /// Runs right after the ledger row is written. An error here is
    /// logged by the caller and never fails the generation or the turn.
    fn call_recorded(&self, _store: &Store, _call: &RecordedCall<'_>) -> Result<(), StoreError> {
        Ok(())
    }
}

pub const ALL_AI_INPUTS: &[AiInput] = AiInput::ALL;

impl AiPolicy for Unlimited {
    fn admit_generation(
        &self,
        _store: &Store,
        _user: UserId,
        _request: AiRequest,
        _now_ms: i64,
    ) -> Result<AiAdmission, StoreError> {
        Ok(AiAdmission::Allowed)
    }

    fn usage(&self, _store: &Store, _user: UserId, _now_ms: i64) -> Result<AiUsage, StoreError> {
        Ok(AiUsage {
            inputs: ALL_AI_INPUTS,
            ..AiUsage::default()
        })
    }
}
