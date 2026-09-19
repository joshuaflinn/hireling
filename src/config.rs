//! Settings read from the process environment.
//!
//! This module exists mostly to demonstrate two conventions: it is a *pure*
//! module (no IO of its own — the environment arrives as a closure), and its
//! tests live in a sibling file rather than at the bottom of this one.
//!
//! Rust 2024 made `std::env::set_var` and `remove_var` `unsafe`, so tests must
//! not mutate the real environment. Env-reading functions are therefore written
//! as a pair: a zero-arg function that reads the real process environment, and
//! a `_from_env` sibling that takes a lookup closure the tests can supply.

use std::ffi::OsString;
use std::path::PathBuf;

/// A lookup into some environment.
///
/// `std::env::var_os` in production, a fixture closure in tests.
pub type EnvLookup<'a> = dyn Fn(&str) -> Option<OsString> + 'a;

/// Application settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Tracing filter directive, e.g. `info` or `myapp=debug`.
    pub log_level: String,
    /// TCP port the HTTP server listens on.
    pub port: u16,
    /// Postgres connection string. Unused until the database work lands; the
    /// health endpoint deliberately never touches it.
    pub database_url: String,
    /// Directory holding the built frontend bundle the server serves.
    pub static_dir: PathBuf,
}

impl Settings {
    /// Read settings from the real process environment.
    ///
    /// # Errors
    ///
    /// Returns an error if a recognized variable is set but not valid UTF-8,
    /// or if `HIRELING_PORT` is not a valid port number.
    pub fn from_process_env() -> anyhow::Result<Self> {
        Self::from_env(&|key| std::env::var_os(key))
    }

    /// Read settings from an arbitrary environment.
    ///
    /// # Errors
    ///
    /// Returns an error if a recognized variable is set but not valid UTF-8,
    /// or if `HIRELING_PORT` is not a valid port number.
    pub fn from_env(get: &EnvLookup<'_>) -> anyhow::Result<Self> {
        let log_level = read_string(get, "RUST_LOG", "info")?;
        let database_url = read_string(
            get,
            "HIRELING_DATABASE_URL",
            "postgres://hireling:hireling@127.0.0.1:5432/hireling",
        )?;
        let static_dir = PathBuf::from(read_string(get, "HIRELING_STATIC_DIR", "web/dist")?);
        let raw_port = read_string(get, "HIRELING_PORT", "3000")?;
        let port = raw_port
            .parse::<u16>()
            .map_err(|err| anyhow::anyhow!("HIRELING_PORT is not a valid port number: {err}"))?;

        Ok(Self {
            log_level,
            port,
            database_url,
            static_dir,
        })
    }
}

/// Read one env var as a UTF-8 string, falling back to `default` when unset.
fn read_string(get: &EnvLookup<'_>, key: &str, default: &str) -> anyhow::Result<String> {
    match get(key) {
        Some(raw) => raw
            .into_string()
            .map_err(|raw| anyhow::anyhow!("{key} is not valid UTF-8: {}", raw.display())),
        None => Ok(default.to_owned()),
    }
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
