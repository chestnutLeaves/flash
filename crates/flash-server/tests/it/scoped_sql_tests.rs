//! Every SQL statement in the open store that touches a per-user table
//! binds `user_id`, or its function is listed here as unscoped by
//! design with the reason. A new query that forgets the user fails this
//! test; a listed function that stops existing fails it too.

use std::path::Path;

use flash_scan::sql::{statements_excluding, violations};

/// Tables whose rows belong to one account. `users` and `invites` are
/// the account tables themselves; `oauth_clients` and
/// `schema_migrations` are global.
const PER_USER_TABLES: &[&str] = &[
    "user_settings",
    "account_locales",
    "webauthn_credentials",
    "sessions",
    "oauth_codes",
    "oauth_tokens",
    "oauth_spent_refresh",
    "mcp_confirmations",
    "decks",
    "cards",
    "tags",
    "card_tags",
    "card_state",
    "review_log",
    "study_sessions",
    "password_resets",
    "media",
    "card_media",
    "import_commits",
    "notes",
    "note_revisions",
    "ai_sources",
    "ai_extractions",
    "ai_figures",
    "ai_jobs",
    "ai_job_sources",
    "ai_turns",
    "ai_drafts",
    "ai_calls",
    "voice_clip_refs",
];

/// Functions whose per-user SQL deliberately carries no user: they are
/// how a user is *found* (a secret proves the caller), housekeeping over
/// every account, content-addressed blob checks, or the admin's view.
const UNSCOPED_BY_DESIGN: &[(&str, &str)] = &[
    // Auth roots: a secret the caller presented is the whole credential;
    // the row it names *is* how the user is found.
    ("lookup_web_session", "session cookie hash → user"),
    ("delete_web_session", "logout by the session's own hash"),
    (
        "touch_web_session",
        "last-used of the session a cookie just proved",
    ),
    ("lookup_password_reset_tx", "reset token hash → user"),
    ("consume_password_reset_tx", "reset token hash, single use"),
    (
        "delete_password_reset",
        "rollback of a reset whose mail failed, by hash",
    ),
    ("consume_oauth_code", "authorization code hash → grant"),
    ("lookup_api_token", "bearer hash → user, client and scope"),
    (
        "rotate_refresh_token",
        "refresh token hash → new pair, the same pair again, or the grant revoked",
    ),
    // Discoverable-credential login: the authenticator names the
    // credential, and its row names the user.
    (
        "passkey_by_cred_id",
        "passkey login: the credential an authenticator named → its owner",
    ),
    ("any_passkey", "whether passkey login is offered at all"),
    // Housekeeping over every account.
    ("purge_expired_password_resets", "scheduler sweep"),
    ("purge_expired_tokens", "scheduler sweep"),
    ("purge_expired_web_sessions", "scheduler sweep"),
    ("purge_expired_oauth_codes", "scheduler sweep"),
    ("purge_import_commits", "scheduler sweep"),
    (
        "sweep_unlinked_media",
        "scheduler sweep of media rows no card links, every account; each delete binds the row's own user",
    ),
    (
        "purge_unused_oauth_clients",
        "scheduler sweep of registrations no grant ever used",
    ),
    // Content-addressed blobs: the question is whether *anyone* holds the hash.
    ("blob_known", "dedupe by hash across users"),
    ("blob_refs", "orphan check by hash across users"),
    (
        "media_owner",
        "a signed study-panel media link: the owner read here is what the signature is checked against",
    ),
    // The admin's view of every account.
    (
        "list_users_admin",
        "admin listing, gated by the admin extractor",
    ),
];

#[test]
fn every_per_user_statement_binds_the_user() {
    // The whole store crate, except import/ and export/, which speak to
    // Anki collections whose tables share some names with ours; and the
    // whole server crate, which may reach the connection through the
    // extension seam and must then scope what it runs.
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flash-store");
    let mut found = statements_excluding(
        &store,
        "src",
        &["src/import/", "src/export"],
        PER_USER_TABLES,
    );
    let server = Path::new(env!("CARGO_MANIFEST_DIR"));
    found.extend(statements_excluding(server, "src", &[], PER_USER_TABLES));
    assert!(
        found.len() > 50,
        "scan found only {} statements",
        found.len()
    );
    let (unscoped, stale) = violations(&found, UNSCOPED_BY_DESIGN);
    assert!(
        unscoped.is_empty(),
        "per-user SQL without user_id (scope it, or list the function with a reason):\n{}",
        unscoped.join("\n")
    );
    assert!(
        stale.is_empty(),
        "listed functions with no unscoped SQL any more (remove them):\n{}",
        stale.join("\n")
    );
}

/// Tables the migrations create that are not per-user: the account
/// tables themselves, the global registries, and a rebuild's transient
/// copy. Every table a migration creates is on exactly one of the two
/// lists, so a table added later has to be placed deliberately rather
/// than forgotten by the scan. `jobs` is the server's own queue: a job
/// may name an account, but only the worker reads it, never a request
/// on someone's behalf. `ceremonies` are found by the hash of a token
/// the browser holds, which is the credential (a consent names its
/// account, checked against the session that finishes it);
/// `mcp_sessions` by the hash of the session id, every call on which
/// still carries the account's own token; `server_state` is the
/// server's own, and so is `id_floor`, the largest id each table has
/// held across every account. `client_errors` counts what broke in
/// browsers and names no account at all.
const NOT_PER_USER: &[&str] = &[
    "ceremonies",
    "client_errors",
    "id_floor",
    "jobs",
    "mcp_sessions",
    "server_state",
    "schema_migrations",
    "users",
    "invites",
    "oauth_clients",
    "review_log_v12",
    "sessions_v027",
    "voice_clips",
];

#[test]
fn every_table_the_migrations_create_is_on_one_of_the_lists() {
    let store = Path::new(env!("CARGO_MANIFEST_DIR")).join("../flash-store");
    let migrations = flash_scan::crate_at(&store).file("src/migrations.rs");
    let mut created = Vec::new();
    for literal in &migrations.string_literals {
        let text = &literal.value;
        let mut from = 0;
        while let Some(pos) = text[from..].find("CREATE TABLE ") {
            let at = from + pos + "CREATE TABLE ".len();
            let rest = text[at..].trim_start_matches("IF NOT EXISTS ");
            let name: String = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect();
            created.push(name);
            from = at;
        }
    }
    assert!(created.len() >= 20, "found {} tables", created.len());
    let mut unlisted = Vec::new();
    for name in &created {
        let per_user = PER_USER_TABLES.contains(&name.as_str());
        let global = NOT_PER_USER.contains(&name.as_str());
        if per_user && global {
            unlisted.push(format!("{name}: on both lists"));
        } else if !per_user && !global {
            unlisted.push(format!("{name}: on neither list"));
        }
    }
    assert!(
        unlisted.is_empty(),
        "tables the scan's lists do not describe:\n{}",
        unlisted.join("\n")
    );
    for listed in PER_USER_TABLES.iter().chain(NOT_PER_USER) {
        assert!(
            created.iter().any(|n| n == listed),
            "{listed} is listed but no migration creates it"
        );
    }
}
