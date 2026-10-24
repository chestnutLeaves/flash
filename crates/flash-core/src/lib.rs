//! Pure domain logic: no IO, no async, no SQLite.

pub mod card;
pub mod day;
pub mod deck_path;
pub mod deck_tree;
pub mod goal;
pub mod ids;
pub mod languages;
pub mod locale;
pub mod queue;
pub mod rating;
pub mod scheduler;
pub mod settings;
pub mod speech;
pub mod text;

pub use card::{validate_card_text, CardText, CardTextError, MAX_SIDE_LEN};
pub use ids::{CardId, DeckId, MediaId, NoteId, RevisionId, SessionId, UserId};
pub use rating::{Phase, Rating};
pub use scheduler::{CardState, ReviewOutcome, Scheduler, SchedulerError};
pub use settings::{GradingMode, UserSettings};
