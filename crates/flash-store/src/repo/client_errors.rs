//! What the website's pages reported going wrong in the browser
//! (`V037_CLIENT_ERRORS`): their own scripts' errors, and the
//! Content-Security-Policy violations browsers reported. Counted per day,
//! kind, script and line, message, page and browser family; nothing
//! about who saw it.

use std::collections::BTreeMap;

use rusqlite::params;

use crate::{Result, Store};

use flash_core::scheduler::MS_PER_DAY as DAY_MS;

/// New rows a day takes; past it only errors already counted that day
/// still count, so a flood of invented messages cannot grow the table.
pub const CLIENT_ERROR_ROWS_PER_DAY: i64 = 2_000;

/// How long an error must have been quiet to be new again.
pub const CLIENT_ERROR_QUIET_MS: i64 = 7 * DAY_MS;

/// One report, already cleaned by the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientErrorNew<'a> {
    /// `error`, `rejection` (a promise nobody caught) or `csp`.
    pub kind: &'a str,
    /// The script's path (`/static/study.js`), or empty.
    pub file: &'a str,
    pub line: u32,
    pub message: &'a str,
    /// The page's path, numbers taken out.
    pub page: &'a str,
    /// The browser's family (`Safari (mobile)`).
    pub browser: &'a str,
}

/// What became of a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorRecorded {
    /// Counted, and not seen in the week before: worth telling someone.
    New,
    /// Counted; seen in the last week.
    Again,
    /// Not counted: the day has its fill of rows.
    Dropped,
}

/// One error (kind, script, line and message) over some days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientErrorSummary {
    pub kind: String,
    pub file: String,
    pub line: u32,
    pub message: String,
    /// Times reported over the days read.
    pub count: u64,
    /// The browser families it came from, sorted.
    pub browsers: Vec<String>,
    /// The pages it came from, most reported first.
    pub pages: Vec<String>,
    pub first_at: i64,
    pub last_at: i64,
}

impl Store {
    /// Counts one report.
    pub fn record_client_error(
        &self,
        error: &ClientErrorNew<'_>,
        now_ms: i64,
    ) -> Result<ClientErrorRecorded> {
        let day = now_ms - now_ms.rem_euclid(DAY_MS);
        let e = error;
        self.with_tx(|tx| {
            let counted = tx.execute(
                "UPDATE client_errors SET count = count + 1, last_at = ?8
                 WHERE day = ?1 AND kind = ?2 AND file = ?3 AND line = ?4
                   AND message = ?5 AND page = ?6 AND browser = ?7",
                params![day, e.kind, e.file, e.line, e.message, e.page, e.browser, now_ms],
            )?;
            if counted > 0 {
                return Ok(ClientErrorRecorded::Again);
            }
            let today: i64 = tx.query_row(
                "SELECT COUNT(*) FROM client_errors WHERE day = ?1",
                [day],
                |r| r.get(0),
            )?;
            if today >= CLIENT_ERROR_ROWS_PER_DAY {
                return Ok(ClientErrorRecorded::Dropped);
            }
            let seen: bool = tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM client_errors
                   WHERE kind = ?1 AND file = ?2 AND line = ?3 AND message = ?4
                     AND last_at >= ?5)",
                params![
                    e.kind,
                    e.file,
                    e.line,
                    e.message,
                    now_ms - CLIENT_ERROR_QUIET_MS
                ],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO client_errors
                   (day, kind, file, line, message, page, browser, count, first_at, last_at,
                    new_error)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?8, ?9)",
                params![day, e.kind, e.file, e.line, e.message, e.page, e.browser, now_ms, !seen],
            )?;
            Ok(if seen {
                ClientErrorRecorded::Again
            } else {
                ClientErrorRecorded::New
            })
        })
    }

    /// Every error reported since `since_ms`, the most reported first.
    pub fn client_errors_since(&self, since_ms: i64) -> Result<Vec<ClientErrorSummary>> {
        self.client_error_summaries(since_ms)
    }

    /// The errors that were new (`ClientErrorRecorded::New`) in
    /// `[from_ms, to_ms)`: the ones nobody has been told about, exactly as
    /// `record_client_error` decided, with their counts since `from_ms`.
    pub fn new_client_errors_between(
        &self,
        from_ms: i64,
        to_ms: i64,
    ) -> Result<Vec<ClientErrorSummary>> {
        let new: Vec<(String, String, u32, String)> = {
            let conn = self.conn();
            let mut stmt = conn.prepare(
                "SELECT DISTINCT kind, file, line, message FROM client_errors
                 WHERE new_error = 1 AND first_at >= ?1 AND first_at < ?2",
            )?;
            let rows = stmt.query_map([from_ms, to_ms], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut errors = self.client_error_summaries(from_ms)?;
        errors.retain(|e| {
            new.iter().any(|(kind, file, line, message)| {
                *kind == e.kind && *file == e.file && *line == e.line && *message == e.message
            })
        });
        Ok(errors)
    }

    /// Rows last counted before `before_ms` go.
    pub fn purge_client_errors(&self, before_ms: i64) -> Result<usize> {
        Ok(self
            .conn()
            .execute("DELETE FROM client_errors WHERE last_at < ?1", [before_ms])?)
    }

    /// The rows counted since `since_ms`, one summary per error.
    fn client_error_summaries(&self, since_ms: i64) -> Result<Vec<ClientErrorSummary>> {
        type Key = (String, String, u32, String);
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT kind, file, line, message, page, browser, count, first_at, last_at
             FROM client_errors WHERE last_at >= ?1",
        )?;
        let rows = stmt.query_map([since_ms], |r| {
            Ok((
                (
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, String>(3)?,
                ),
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, i64>(6)?.max(0) as u64,
                r.get::<_, i64>(7)?,
                r.get::<_, i64>(8)?,
            ))
        })?;
        let mut by_error: BTreeMap<Key, (ClientErrorSummary, BTreeMap<String, u64>)> =
            BTreeMap::new();
        for row in rows {
            let ((kind, file, line, message), page, browser, count, first_at, last_at) = row?;
            let (summary, pages) = by_error
                .entry((kind.clone(), file.clone(), line, message.clone()))
                .or_insert_with(|| {
                    (
                        ClientErrorSummary {
                            kind,
                            file,
                            line,
                            message,
                            count: 0,
                            browsers: Vec::new(),
                            pages: Vec::new(),
                            first_at,
                            last_at,
                        },
                        BTreeMap::new(),
                    )
                });
            summary.count += count;
            summary.first_at = summary.first_at.min(first_at);
            summary.last_at = summary.last_at.max(last_at);
            if !summary.browsers.contains(&browser) {
                summary.browsers.push(browser);
            }
            *pages.entry(page).or_default() += count;
        }
        let mut errors: Vec<ClientErrorSummary> = by_error
            .into_values()
            .map(|(mut summary, pages)| {
                summary.browsers.sort();
                let mut pages: Vec<(String, u64)> = pages.into_iter().collect();
                pages.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                summary.pages = pages.into_iter().map(|(page, _)| page).collect();
                summary
            })
            .collect();
        errors.sort_by(|a, b| {
            b.count
                .cmp(&a.count)
                .then_with(|| b.last_at.cmp(&a.last_at))
        });
        Ok(errors)
    }
}
