//! The jobs table (`V029_JOBS`): work owed to someone, written before it
//! is done. Enqueue inside the transaction that makes the promise
//! (`enqueue_job` takes any connection or transaction); a worker claims
//! due jobs under a lease and reports each one done, retried or failed.

use flash_core::UserId;
use rusqlite::{params, Connection, OptionalExtension};

use crate::{Result, Store};

/// A job to write down.
#[derive(Debug, Clone)]
pub struct NewJob<'a> {
    /// Which handler runs it.
    pub kind: &'a str,
    /// A second enqueue with the same key is a no-op: the promise is made
    /// once however often the code that makes it runs.
    pub dedupe_key: Option<&'a str>,
    /// Whose job it is, when it is someone's; their jobs go with them.
    pub user: Option<UserId>,
    /// What the handler needs, as JSON.
    pub payload: &'a str,
    /// Not before this instant.
    pub run_at: i64,
}

/// A claimed job, as its handler gets it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRow {
    pub id: i64,
    pub kind: String,
    pub dedupe_key: Option<String>,
    pub user: Option<UserId>,
    pub payload: String,
    /// Claims so far, this one included.
    pub attempts: u32,
}

/// Where one job stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobState {
    pub done_at: Option<i64>,
    pub failed_at: Option<i64>,
    /// Why the last try failed, if one did.
    pub last_error: Option<String>,
}

/// How many jobs are waiting, being retried, and failed for good.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobCounts {
    pub due: u32,
    pub retrying: u32,
    pub failed: u32,
}

/// The job that removes blobs a deletion left unreferenced: its payload
/// is the list of hashes. The store writes it inside the deletion's own
/// transaction; the server's worker deletes them from storage.
pub const JOB_DELETE_BLOBS: &str = "delete_blobs";

/// Owes the storage deletes of `hashes`, on the deletion's transaction.
pub fn owe_blob_deletes(conn: &Connection, hashes: &[String]) -> Result<()> {
    if hashes.is_empty() {
        return Ok(());
    }
    let now = crate::migrations::now_ms();
    let payload = serde_json::to_string(hashes).unwrap_or_else(|_| "[]".into());
    enqueue_job(
        conn,
        &NewJob {
            kind: JOB_DELETE_BLOBS,
            dedupe_key: None,
            // Owed to nobody: an account's blobs outlive the account.
            user: None,
            payload: &payload,
            run_at: now,
        },
        now,
    )?;
    Ok(())
}

/// `Store::job_counts` on `conn`, for a report that reads its figures on
/// one connection.
pub fn job_counts_tx(conn: &Connection, now_ms: i64) -> Result<JobCounts> {
    Ok(conn.query_row(
        "SELECT
           COUNT(CASE WHEN done_at IS NULL AND failed_at IS NULL AND run_at <= ?1
                      AND attempts = 0 THEN 1 END),
           COUNT(CASE WHEN done_at IS NULL AND failed_at IS NULL AND attempts > 0
                      THEN 1 END),
           COUNT(CASE WHEN failed_at IS NOT NULL THEN 1 END)
         FROM jobs",
        [now_ms],
        |r| {
            Ok(JobCounts {
                due: r.get::<_, i64>(0)?.max(0) as u32,
                retrying: r.get::<_, i64>(1)?.max(0) as u32,
                failed: r.get::<_, i64>(2)?.max(0) as u32,
            })
        },
    )?)
}

/// Writes `job` on `conn`, which may be a transaction the caller is in.
/// False when a job with the same `dedupe_key` already exists.
pub fn enqueue_job(conn: &Connection, job: &NewJob<'_>, now_ms: i64) -> Result<bool> {
    let changed = conn.execute(
        "INSERT INTO jobs (kind, dedupe_key, user_id, payload, run_at, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(dedupe_key) DO NOTHING",
        params![
            job.kind,
            job.dedupe_key,
            job.user.map(|u| u.raw()),
            job.payload,
            job.run_at,
            now_ms
        ],
    )?;
    Ok(changed > 0)
}

impl Store {
    /// `enqueue_job` on its own, for a promise made outside a
    /// transaction.
    pub fn enqueue_job(&self, job: &NewJob<'_>, now_ms: i64) -> Result<bool> {
        enqueue_job(&self.conn(), job, now_ms)
    }

    /// Writes `job` unless a job whose `dedupe_key` starts with
    /// `key_prefix` was written after `since_ms`. One statement, so two
    /// writers racing for the same window write one job. False when
    /// nothing was written.
    pub fn enqueue_job_unless_recent(
        &self,
        job: &NewJob<'_>,
        key_prefix: &str,
        since_ms: i64,
        now_ms: i64,
    ) -> Result<bool> {
        let changed = self.conn().execute(
            "INSERT INTO jobs (kind, dedupe_key, user_id, payload, run_at, created_at)
             SELECT ?1, ?2, ?3, ?4, ?5, ?6
             WHERE NOT EXISTS (SELECT 1 FROM jobs
                               WHERE substr(dedupe_key, 1, length(?7)) = ?7 AND created_at > ?8)
             ON CONFLICT(dedupe_key) DO NOTHING",
            params![
                job.kind,
                job.dedupe_key,
                job.user.map(|u| u.raw()),
                job.payload,
                job.run_at,
                now_ms,
                key_prefix,
                since_ms
            ],
        )?;
        Ok(changed > 0)
    }

    /// Takes up to `limit` due jobs, oldest first, under a lease until
    /// `now_ms + lease_ms`: a job is due when it is neither done nor
    /// failed, its time has come, and no live lease holds it. One
    /// statement, so two workers never take the same job.
    pub fn claim_due_jobs(&self, now_ms: i64, lease_ms: i64, limit: u32) -> Result<Vec<JobRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "UPDATE jobs SET lease_until = ?1 + ?2, attempts = attempts + 1
             WHERE id IN (
               SELECT id FROM jobs
               WHERE done_at IS NULL AND failed_at IS NULL AND run_at <= ?1
                 AND (lease_until IS NULL OR lease_until <= ?1)
               ORDER BY run_at, id LIMIT ?3)
             RETURNING id, kind, dedupe_key, user_id, payload, attempts",
        )?;
        let mut rows = stmt
            .query_map(params![now_ms, lease_ms, limit], |r| {
                Ok(JobRow {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    dedupe_key: r.get(2)?,
                    user: r.get::<_, Option<i64>>(3)?.map(UserId::from_db),
                    payload: r.get(4)?,
                    attempts: r.get::<_, i64>(5)?.max(0) as u32,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // RETURNING gives no order; the oldest runs first.
        rows.sort_by_key(|j| j.id);
        Ok(rows)
    }

    /// Done. The payload goes: some carry a secret (a token to revoke)
    /// that nothing needs once the work is done.
    pub fn finish_job(&self, id: i64, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE jobs SET done_at = ?2, lease_until = NULL, payload = '{}' WHERE id = ?1",
            params![id, now_ms],
        )?;
        Ok(())
    }

    /// Back to waiting, not before `run_at`, noting why this try failed.
    pub fn retry_job(&self, id: i64, run_at: i64, error: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE jobs SET run_at = ?2, lease_until = NULL, last_error = ?3 WHERE id = ?1",
            params![id, run_at, error],
        )?;
        Ok(())
    }

    /// Given up on: kept, with why, for someone to look at.
    pub fn fail_job(&self, id: i64, now_ms: i64, error: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE jobs SET failed_at = ?2, lease_until = NULL, last_error = ?3 WHERE id = ?1",
            params![id, now_ms, error],
        )?;
        Ok(())
    }

    /// Jobs done before `before_ms` go; failed ones stay until someone
    /// has looked.
    pub fn purge_done_jobs(&self, before_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM jobs WHERE done_at IS NOT NULL AND done_at < ?1",
            [before_ms],
        )?)
    }

    /// The jobs that failed for good at or after `start_ms` and before
    /// `end_ms`, oldest first: (kind, last error, when).
    pub fn failed_jobs_between(
        &self,
        start_ms: i64,
        end_ms: i64,
    ) -> Result<Vec<(String, String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare(
            "SELECT kind, COALESCE(last_error, ''), failed_at FROM jobs
             WHERE failed_at >= ?1 AND failed_at < ?2 ORDER BY failed_at, id",
        )?;
        let rows = stmt.query_map([start_ms, end_ms], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Jobs that failed before `before_ms` go, payload and all: long
    /// enough ago that anyone who was going to look has.
    pub fn purge_failed_jobs(&self, before_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "DELETE FROM jobs WHERE failed_at IS NOT NULL AND failed_at < ?1",
            [before_ms],
        )?)
    }

    /// Whether nothing (no media row, nothing an extension holds) names
    /// blob `sha256` any more.
    pub fn blob_unreferenced(&self, sha256: &str) -> Result<bool> {
        Ok(self.blob_refs(&self.conn(), sha256)? == 0)
    }

    /// What the queue looks like right now.
    pub fn job_counts(&self, now_ms: i64) -> Result<JobCounts> {
        job_counts_tx(&self.conn(), now_ms)
    }

    /// Where one job stands.
    pub fn job_state(&self, id: i64) -> Result<Option<JobState>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT done_at, failed_at, last_error FROM jobs WHERE id = ?1",
                [id],
                |r| {
                    Ok(JobState {
                        done_at: r.get(0)?,
                        failed_at: r.get(1)?,
                        last_error: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }
}
