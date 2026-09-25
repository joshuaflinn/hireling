//! The binary is a thin shell over this library.
//!
//! Keeping the work here rather than in `main.rs` means it can be tested
//! without spawning a process. As the project grows, the shape to hold is:
//! IO-bound and framework-bound code (HTTP handlers, event loops, GUI
//! callbacks) stays in a thin *shell* layer that calls into *pure* modules
//! where the logic lives. The shell is usually not unit-tested; the pure
//! modules always are.

pub mod auth;
pub mod config;
pub mod db;
pub mod health;
pub mod http;

#[cfg(test)]
pub mod testing;

use anyhow::Context as _;

use crate::config::Settings;

/// Run the application: load settings, connect to Postgres and migrate, then
/// serve until shutdown.
///
/// # Errors
///
/// Returns an error if settings cannot be read from the environment, the
/// database cannot be reached or migrated, the port cannot be bound, or the
/// server fails.
pub async fn run() -> anyhow::Result<()> {
    let settings = Settings::from_process_env().context("failed to load settings")?;

    // Fail closed on incomplete release configuration: a release binary that
    // boots without its OIDC legs and cookie key is an app nobody can log
    // into. Debug builds may run without them (dev-session auth stands in).
    if cfg!(not(debug_assertions)) {
        settings
            .auth
            .validate_release()
            .context("release configuration incomplete")?;
    }

    tracing::info!(
        log_level = %settings.log_level,
        version = health::VERSION,
        "starting"
    );

    // Schema before traffic: a database the app can't reach or migrate fails
    // the boot here, never at first query. The pool lives for the server's
    // lifetime; the first pool-consuming endpoint (E5) moves it into router
    // state.
    let pool = db::connect_and_migrate(&settings.database_url).await?;

    http::serve(&settings, pool).await
}
