//! The seam a downstream crate extends the store through. The open
//! store owns accounts, decks, cards, reviews, media and tokens; an
//! extension owns its own tables on the same connection (created by its
//! own migrations) and is consulted where a core deletion would
//! otherwise leave its rows dangling.
//!
//! Hooks receive the core's own transaction (a `&Transaction` derefs to
//! `&Connection`), so whatever they do commits or rolls back with it.
//! They must use only the connection they are handed: the store's mutex
//! is held for the duration.

use rusqlite::Connection;

use flash_core::{DeckId, UserId};

use crate::Result;

pub use crate::exec_expect_row;
pub use crate::repo::cards::{attach_tag, card_tags, set_card_words, write_card_extras};
pub use crate::repo::decks::require_deck;

/// A deck's name is its path, so a deck is also the decks under it. An
/// extension that acts on "a deck" reads the subtree with `deck_subtree`,
/// looks a path up with `find_deck_path`, picks a name no deck has with
/// `free_deck_path`, and makes decks with `create_deck_path`, which
/// holds them to the rules every other way of making a deck meets (and
/// makes the decks on the way, so a path made after one under it is
/// found, not made twice).
pub use crate::repo::decks::{
    find_deck as find_deck_path, free_deck_path, insert_deck_path_tx as create_deck_path,
    subtree as deck_subtree,
};

/// The core's row writers, each the one statement (or the few) the core
/// itself writes a card, its media links, a deck's name, a review or a
/// card's state with, for an extension that replays edits made
/// elsewhere inside its own transaction. `Store::create_note_tx`,
/// `update_note_tx`, `delete_card_tx` and `delete_deck_tx` do the same
/// for notes, a card's deletion and a deck's. Every one is ownership
/// guarded and holds the rules the core's own callers meet.
pub use crate::repo::cards::{insert_card, link_card_media_tx, suspend_card, unlink_card_media};
pub use crate::repo::deck_goals::deck_goal_paths;
pub use crate::repo::decks::rename_deck_tx;
pub use crate::repo::reviews::{insert_review, write_card_state};

/// One schema step: a unique name (recorded once applied) and the SQL
/// that runs, in its own transaction, the first time the extension opens
/// a database that lacks it.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    pub name: &'static str,
    pub sql: &'static str,
}

pub trait StoreExtension: Send + Sync + 'static {
    /// The key the extension's migrations are recorded under.
    fn name(&self) -> &'static str;

    /// The extension's schema history, in order, append-only.
    fn migrations(&self) -> &'static [Migration] {
        &[]
    }

    /// How many leading migrations a database from before the schema
    /// registry already contains: those are recorded as applied without
    /// running when such a database is first opened.
    fn legacy_baseline(&self) -> usize {
        0
    }

    /// A query answering 1 when a database already holds what `step`
    /// makes (made before the step was this extension's); the step is
    /// then recorded as applied without running.
    fn adopt_if(&self, _step: &str) -> Option<&'static str> {
        None
    }

    /// Runs inside `delete_deck`, after the deck's cards and notes are
    /// gone and before the deck row is. `user` owns the deck (the core
    /// checked); the extension's own rows must be scoped by it too.
    fn before_delete_deck(&self, _conn: &Connection, _user: UserId, _deck: DeckId) -> Result<()> {
        Ok(())
    }

    /// Runs inside `delete_user_account`, before the core deletes the
    /// account's own rows.
    fn before_delete_user(&self, _conn: &Connection, _user: UserId) -> Result<()> {
        Ok(())
    }

    /// Runs inside `delete_sessions_for_user`, where the core signs a
    /// user out of every session but perhaps the one asking (a password
    /// reset or change, signing out everywhere else). Whatever of the
    /// extension's own would sign the user in again must end there too.
    fn before_delete_sessions(&self, _conn: &Connection, _user: UserId) -> Result<()> {
        Ok(())
    }

    /// How many rows of the extension's own still name this blob hash.
    /// A blob is removed from storage only when the core's count and
    /// every extension's count are all zero.
    fn blob_refs(&self, _conn: &Connection, _sha256: &str) -> Result<i64> {
        Ok(0)
    }
}
