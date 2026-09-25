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

use std::collections::HashMap;
use std::collections::HashSet;
use std::ffi::OsString;
use std::path::PathBuf;

use crate::auth::oidc;

/// A lookup into some environment.
///
/// `std::env::var_os` in production, a fixture closure in tests.
pub type EnvLookup<'a> = dyn Fn(&str) -> Option<OsString> + 'a;

/// Identity-provider settings. Present only when the OIDC environment is
/// fully configured; its absence is survivable in debug builds only (the
/// compile-gated dev-session route stands in).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcSettings {
    /// Issuer base, e.g. `https://auth.flinntech.com/application/o/hireling/`.
    pub issuer: String,
    /// Authentik confidential-client id.
    pub client_id: String,
    /// Authentik confidential-client secret.
    pub client_secret: String,
    /// Public origin, e.g. `https://hireling.flinntech.com`.
    pub base_url: String,
    /// The authorize/token/JWKS endpoints, derived from the issuer.
    pub urls: oidc::ProviderUrls,
}

impl OidcSettings {
    /// The exact redirect URI the provider must have registered.
    #[must_use]
    pub fn redirect_uri(&self) -> String {
        format!("{}/api/auth/callback", self.base_url.trim_end_matches('/'))
    }
}

/// Everything the auth layers and legs need from configuration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthSettings {
    /// OIDC provider settings; `None` only in debug builds.
    pub oidc: Option<OidcSettings>,
    /// 32-byte master key for the signed transaction cookie (from 64 hex
    /// chars). The in-use [`cookie::Key`] is derived from this at startup.
    pub cookie_key: Vec<u8>,
    /// The provider `sub`s allowed to hold a session (the six accounts).
    pub allowlist: HashSet<String>,
    /// Seat-name fallbacks: `sub` → name, for entries written `sub=name`.
    pub seat_names: HashMap<String, String>,
    /// The allowlisted `sub` holding the GM seat (Bruce).
    pub gm_sub: String,
    /// Sliding idle window for sessions, seconds.
    pub session_idle_secs: i64,
    /// Hard absolute session cap, seconds.
    pub session_absolute_secs: i64,
}

impl AuthSettings {
    /// Parse the auth settings from an environment.
    ///
    /// The allowlist is always required (an app that can name no accounts is
    /// a misconfiguration, not a mode). The OIDC five are required in release
    /// builds and optional as a complete group in debug builds.
    ///
    /// # Errors
    ///
    /// Returns an error for missing required variables, malformed values,
    /// non-positive session windows, or a GM `sub` outside the allowlist.
    pub fn from_env(get: &EnvLookup<'_>) -> anyhow::Result<Self> {
        let allowlist_raw = read_string(get, "HIRELING_ALLOWLIST", "")?;
        if allowlist_raw.is_empty() {
            anyhow::bail!("HIRELING_ALLOWLIST is required (comma-separated provider subs)");
        }
        let mut allowlist = HashSet::new();
        let mut seat_names = HashMap::new();
        for entry in allowlist_raw.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            match entry.split_once('=') {
                Some((sub, name)) => {
                    allowlist.insert(sub.to_owned());
                    seat_names.insert(sub.to_owned(), name.to_owned());
                }
                None => {
                    allowlist.insert(entry.to_owned());
                }
            }
        }

        let gm_sub = read_string(get, "HIRELING_GM_SUB", "")?;
        if !allowlist.contains(&gm_sub) {
            anyhow::bail!("HIRELING_GM_SUB must be one of HIRELING_ALLOWLIST");
        }

        let session_idle_secs = read_number(
            get,
            "HIRELING_SESSION_IDLE_SECS",
            crate::auth::session::DEFAULT_IDLE_SECS,
        )?;
        let session_absolute_secs = read_number(
            get,
            "HIRELING_SESSION_ABSOLUTE_SECS",
            crate::auth::session::DEFAULT_ABSOLUTE_SECS,
        )?;
        if session_idle_secs <= 0 || session_absolute_secs <= 0 {
            anyhow::bail!("session windows must be positive");
        }
        if session_absolute_secs < session_idle_secs {
            anyhow::bail!(
                "HIRELING_SESSION_ABSOLUTE_SECS ({session_absolute_secs}) must not be \
                 shorter than HIRELING_SESSION_IDLE_SECS ({session_idle_secs})"
            );
        }

        let oidc = oidc_settings_from_env(get)?;
        let cookie_key = parse_cookie_key(get)?;

        Ok(Self {
            oidc,
            cookie_key,
            allowlist,
            seat_names,
            gm_sub,
            session_idle_secs,
            session_absolute_secs,
        })
    }

    /// The seat-name fallback for a `sub`, if the allowlist config paired one.
    pub fn seat_name(&self, sub: &str) -> Option<&str> {
        self.seat_names.get(sub).map(String::as_str)
    }

    /// Release-boot validation: the OIDC five and the cookie key must be
    /// present. Debug builds may run without them (dev-session auth).
    ///
    /// # Errors
    ///
    /// Returns an error naming the first missing piece.
    pub fn validate_release(&self) -> anyhow::Result<()> {
        if self.oidc.is_none() {
            anyhow::bail!(
                "HIRELING_OIDC_ISSUER, HIRELING_OIDC_CLIENT_ID, HIRELING_OIDC_CLIENT_SECRET, \
                 and HIRELING_BASE_URL are required in release builds"
            );
        }
        if self.cookie_key.len() != 32 {
            anyhow::bail!("HIRELING_COOKIE_KEY must be 64 hex chars");
        }
        Ok(())
    }
}

/// Parse the five OIDC variables as a complete group: any of them present
/// requires all of them, so a typoed variable fails the boot instead of
/// silently disabling the login legs.
fn oidc_settings_from_env(get: &EnvLookup<'_>) -> anyhow::Result<Option<OidcSettings>> {
    let issuer = read_string(get, "HIRELING_OIDC_ISSUER", "")?;
    let client_id = read_string(get, "HIRELING_OIDC_CLIENT_ID", "")?;
    let client_secret = read_string(get, "HIRELING_OIDC_CLIENT_SECRET", "")?;
    let base_url = read_string(get, "HIRELING_BASE_URL", "")?;
    if issuer.is_empty() && client_id.is_empty() && client_secret.is_empty() && base_url.is_empty()
    {
        return Ok(None);
    }
    if issuer.is_empty() || client_id.is_empty() || client_secret.is_empty() || base_url.is_empty()
    {
        anyhow::bail!(
            "HIRELING_OIDC_ISSUER, HIRELING_OIDC_CLIENT_ID, HIRELING_OIDC_CLIENT_SECRET, \
             and HIRELING_BASE_URL must be configured together"
        );
    }
    let urls = oidc::derive_provider_urls(&issuer)?;
    Ok(Some(OidcSettings {
        issuer,
        client_id,
        client_secret,
        base_url,
        urls,
    }))
}

/// Decode `HIRELING_COOKIE_KEY` (64 hex chars) into its 32 bytes.
fn parse_cookie_key(get: &EnvLookup<'_>) -> anyhow::Result<Vec<u8>> {
    let hex = read_string(get, "HIRELING_COOKIE_KEY", "")?;
    if hex.is_empty() {
        return Ok(Vec::new());
    }
    if hex.len() != 64 {
        anyhow::bail!(
            "HIRELING_COOKIE_KEY must be 64 hex chars, got {}",
            hex.len()
        );
    }
    let bytes: Vec<u8> = (0..32)
        .map(|i| {
            // The length check above bounds the slice; `get` keeps it honest.
            let pair = hex
                .get(i * 2..i * 2 + 2)
                .ok_or_else(|| anyhow::anyhow!("HIRELING_COOKIE_KEY is not 64 hex chars"))?;
            u8::from_str_radix(pair, 16)
                .map_err(|err| anyhow::anyhow!("HIRELING_COOKIE_KEY is not hex: {err}"))
        })
        .collect::<Result<Vec<u8>, anyhow::Error>>()?;
    Ok(bytes)
}

/// Application settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Tracing filter directive, e.g. `info` or `myapp=debug`.
    pub log_level: String,
    /// TCP port the HTTP server listens on.
    pub port: u16,
    /// Postgres connection string; sessions and audit live here.
    pub database_url: String,
    /// Directory holding the built frontend bundle the server serves.
    pub static_dir: PathBuf,
    /// Authentication and authorization settings.
    pub auth: AuthSettings,
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
        let auth = AuthSettings::from_env(get)?;

        Ok(Self {
            log_level,
            port,
            database_url,
            static_dir,
            auth,
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

/// Read one env var as a number, falling back to `default` when unset.
fn read_number(get: &EnvLookup<'_>, key: &str, default: i64) -> anyhow::Result<i64> {
    let raw = read_string(get, key, "")?;
    if raw.is_empty() {
        return Ok(default);
    }
    raw.parse::<i64>()
        .map_err(|err| anyhow::anyhow!("{key} is not a valid number: {err}"))
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;
