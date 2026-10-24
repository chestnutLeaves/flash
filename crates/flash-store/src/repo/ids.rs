//! Ids that are never handed out twice.
//!
//! A deck, note or card id leaves the server: a phone keeps it to name
//! the row in its next push, and an assistant or an open page holds it
//! too. SQLite gives a new row one past the largest id in its table, so
//! deleting the newest rows (a deck deletion removes its rows outright)
//! would give their ids to the next rows made, and whoever still held
//! an old id would edit, grade or delete a row they never saw. The
//! review log, which outlives its cards, would hand the new card the old
//! one's history. An account's id leaves too, in emailed links and as
//! the name of a phone's folder, so a link would act on a stranger.
//! `id_floor` keeps the largest id each table has held, raised by a
//! trigger on every deletion, and every insert takes its id past it.

use rusqlite::Connection;

use crate::Result;

#[derive(Debug, Clone, Copy)]
pub enum IdTable {
    Users,
    Decks,
    Notes,
    Cards,
}

impl IdTable {
    fn name(self) -> &'static str {
        match self {
            IdTable::Users => "users",
            IdTable::Decks => "decks",
            IdTable::Notes => "notes",
            IdTable::Cards => "cards",
        }
    }
}

/// The id for a new row of `table`: past every id it has ever held.
/// Read and used in the same transaction, as the one writer.
pub fn next_id(conn: &Connection, table: IdTable) -> Result<i64> {
    let name = table.name();
    Ok(conn.query_row(
        &format!(
            "SELECT MAX(COALESCE((SELECT MAX(id) FROM {name}), 0),
                        COALESCE((SELECT max_id FROM id_floor WHERE tbl = ?1), 0)) + 1"
        ),
        [name],
        |r| r.get(0),
    )?)
}
