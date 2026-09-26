//! The binary is a thin shell over this library.
//!
//! Keeping the work here rather than in `main.rs` means it can be tested
//! without spawning a process. As the project grows, the shape to hold is:
//! IO-bound and framework-bound code (HTTP handlers, event loops, GUI
//! callbacks) stays in a thin *shell* layer that calls into *pure* modules
//! where the logic lives. The shell is usually not unit-tested; the pure
//! modules always are.

pub mod config;
pub mod db;
pub mod health;
pub mod http;
pub mod import;

use std::process::ExitCode;

use anyhow::Context as _;

use crate::config::Settings;
use crate::import::args::Command;

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

/// Parse command-line arguments and dispatch to the matching mode.
///
/// No arguments means the default mode: serve the app. The importer,
/// license archive, and license verdict subcommands are operator jobs —
/// they run once against a target database and exit.
///
/// `args` excludes argv[0] and must be UTF-8 (the binary collects it via
/// [`std::env::args`], which enforces that).
///
/// # Errors
///
/// Returns an error for unusable arguments or a failing operator job;
/// the caller maps errors to the process exit code.
pub async fn dispatch(args: &[String]) -> anyhow::Result<ExitCode> {
    match import::args::parse(args)? {
        Command::Serve => run().await.map(|()| ExitCode::SUCCESS),
        Command::Usage(text) => {
            println!("{text}");
            Ok(ExitCode::SUCCESS)
        }
        Command::Import { release } => {
            let settings = Settings::from_process_env().context("failed to load settings")?;
            import::run_import(&settings, &release).await?;
            Ok(ExitCode::SUCCESS)
        }
        Command::LicenseVerdict => {
            let settings = Settings::from_process_env().context("failed to load settings")?;
            let green = import::run_license_verdict(&settings).await?;
            if green {
                Ok(ExitCode::SUCCESS)
            } else {
                Ok(ExitCode::FAILURE)
            }
        }
        Command::LicenseArchive { release } => {
            import::run_license_archive(&release).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}
