//! Every integration test of this crate, linked into one binary. One test
//! binary per file cost a link and a few hundred megabytes each; one binary
//! costs one. Each file is a module, so a filter picks a file or a test:
//!
//!   cargo test -p flash-server --test it oauth_tests::
//!   cargo test -p flash-server --test it oauth_tests::a_name

mod common;

// The build script's own code, tested here: a build script runs no tests.
#[allow(dead_code, reason = "the tests call only what they test")]
#[path = "../../build/literals.rs"]
mod literals;
#[allow(dead_code, reason = "the tests call only what they test")]
#[path = "../../build/physical.rs"]
mod physical;
#[allow(dead_code, reason = "the build reads only the table's rows")]
#[path = "../../../flash-core/src/locale/registry.rs"]
mod registry;
#[allow(dead_code, reason = "the tests call only what they test")]
#[path = "../../build/tree.rs"]
mod tree;

mod access_log_tests;
mod account_locale_tests;
mod account_tests;
mod api_admin_tests;
mod api_auth_tests;
mod api_core_tests;
mod api_decks_cards_tests;
mod api_import_tests;
mod api_media_tests;
mod api_onboarding_tests;
mod api_passkey_tests;
mod api_settings_tests;
mod api_stats_tests;
mod api_study_tests;
mod atomic_writes_tests;
mod bidi_vectors_tests;
mod bounded_input_tests;
mod bounds_enforcement_tests;
mod build_literals_tests;
mod build_physical_tests;
mod client_errors_tests;
mod connected_apps_tests;
mod core_only_tests;
mod deck_bulk_tests;
mod deck_pages_tests;
mod devices_tests;
mod docker_image_tests;
mod durable_work_tests;
mod editor_tests;
mod error_surface_tests;
mod global_state_tests;
mod goal_tests;
mod heavy_tests;
mod i18n_tests;
mod import_media_http_tests;
mod isolation_tests;
mod link_session_tests;
mod list_vectors_tests;
mod lock_hygiene_tests;
mod log_hygiene_tests;
mod mcp_guard_tests;
mod mcp_panel_tests;
mod mcp_tests;
mod media_tests;
mod memory_host_tests;
mod oauth_tests;
mod panic_containment_tests;
mod passkey_ceremony_tests;
mod password_tests;
mod picker_vectors_tests;
mod plural_rules_tests;
mod preview_tests;
mod pseudo_vectors_tests;
mod rate_limit_inventory_tests;
mod reset_hygiene_tests;
mod restart_survival_tests;
mod rich_html_boundary_tests;
mod route_inventory_tests;
mod scoped_sql_tests;
mod script_css_tests;
mod service_tests;
mod shutdown_tests;
mod stats_page_tests;
mod study_edit_tests;
mod study_page_tests;
mod test_hygiene_tests;
mod test_layout_tests;
mod timezone_tests;
mod user_id_boundary_tests;
