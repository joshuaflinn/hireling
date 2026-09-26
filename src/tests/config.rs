//! Tests for [`super`].
//!
//! Loaded by `src/config.rs` via `#[path = "tests/config.rs"] mod tests;`, which
//! makes this a *child* of that module — so `use super::*` reaches private items
//! exactly as an inline `mod tests` would. No visibility inflation, no test
//! noise in the impl file.
//!
//! Note there is no `#[expect(clippy::unwrap_used, ...)]` header here. The root
//! `clippy.toml` already exempts unwrap in tests, so an expectation would never
//! fire and would itself become an unfulfilled-expectation error.

use std::ffi::OsString;

use super::Settings;

/// Build a lookup closure over a fixed set of pairs.
fn env_with(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
    let owned: Vec<(String, String)> = pairs
        .iter()
        .map(|&(key, value)| (key.to_owned(), value.to_owned()))
        .collect();

    move |key| {
        owned
            .iter()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, value)| OsString::from(value))
    }
}

/// The auth variables every full-settings fixture needs.
fn auth_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("HIRELING_ALLOWLIST", "sub-a,sub-b"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ]
}

#[test]
fn log_level_defaults_to_info_when_unset() {
    let settings = Settings::from_env(&env_with(&auth_env())).unwrap();

    assert_eq!(
        settings.log_level, "info",
        "an unset RUST_LOG should fall back to info"
    );
}

#[test]
fn log_level_is_read_from_the_environment() {
    let mut env = auth_env();
    env.push(("RUST_LOG", "myapp=debug"));
    let settings = Settings::from_env(&env_with(&env)).unwrap();

    assert_eq!(
        settings.log_level, "myapp=debug",
        "RUST_LOG should override the default"
    );
}

#[test]
fn non_utf8_log_level_is_an_error() {
    let result = Settings::from_env(&|_| Some(invalid_utf8()));

    assert!(
        result.is_err(),
        "a RUST_LOG that is not valid UTF-8 should be reported, not silently dropped"
    );
}

#[test]
fn server_settings_have_dev_defaults() {
    let settings = Settings::from_env(&env_with(&auth_env())).unwrap();

    assert_eq!(settings.port, 3000, "default port should be 3000");
    assert_eq!(
        settings.database_url, "postgres://hireling:hireling@127.0.0.1:5432/hireling",
        "default database URL should point at the local throwaway Postgres"
    );
    assert_eq!(
        settings.static_dir,
        std::path::Path::new("web/dist"),
        "default static dir should be the frontend build output"
    );
}

#[test]
fn server_settings_are_read_from_the_environment() {
    let mut env = auth_env();
    env.push(("HIRELING_PORT", "8080"));
    env.push(("HIRELING_DATABASE_URL", "postgres://example/hireling"));
    env.push(("HIRELING_STATIC_DIR", "/srv/hireling/static"));
    let settings = Settings::from_env(&env_with(&env)).unwrap();

    assert_eq!(settings.port, 8080);
    assert_eq!(settings.database_url, "postgres://example/hireling");
    assert_eq!(
        settings.static_dir,
        std::path::Path::new("/srv/hireling/static")
    );
}

#[test]
fn a_non_numeric_port_is_an_error() {
    let mut env = auth_env();
    env.push(("HIRELING_PORT", "not-a-port"));
    let result = Settings::from_env(&env_with(&env));

    assert!(
        result.is_err(),
        "a HIRELING_PORT that is not a number should be reported, not silently dropped"
    );
}

// --- auth settings ----------------------------------------------------------

#[test]
fn a_missing_allowlist_is_a_boot_error() {
    let result = Settings::from_env(&env_with(&[]));
    let err = format!("{}", result.unwrap_err());
    assert!(
        err.contains("HIRELING_ALLOWLIST"),
        "the error names the missing variable: {err}"
    );
}

#[test]
fn a_gm_sub_outside_the_allowlist_is_a_boot_error() {
    let env = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-not-listed"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ];
    let result = Settings::from_env(&env_with(&env));
    let err = format!("{}", result.unwrap_err());
    assert!(err.contains("HIRELING_GM_SUB"), "error: {err}");
}

#[test]
fn seat_names_are_parsed_from_sub_name_entries() {
    let env = vec![
        ("HIRELING_ALLOWLIST", "sub-a=Josh,sub-b"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ];
    let settings = Settings::from_env(&env_with(&env)).unwrap();
    assert!(settings.auth.allowlist.contains("sub-a"));
    assert!(settings.auth.allowlist.contains("sub-b"));
    assert_eq!(settings.auth.seat_name("sub-a"), Some("Josh"));
    assert_eq!(settings.auth.seat_name("sub-b"), None);
}

#[test]
fn the_cookie_key_is_decoded_from_hex() {
    let env = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
        ),
    ];
    let settings = Settings::from_env(&env_with(&env)).unwrap();
    assert_eq!(settings.auth.cookie_key, vec![0xc3; 32]);

    let bad_hex = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        ("HIRELING_COOKIE_KEY", "zz"),
    ];
    assert!(
        Settings::from_env(&env_with(&bad_hex)).is_err(),
        "non-hex key is an error"
    );
    let short_key = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        ("HIRELING_COOKIE_KEY", "abcd"),
    ];
    assert!(
        Settings::from_env(&env_with(&short_key)).is_err(),
        "short key is an error"
    );
}

#[test]
fn session_windows_validate() {
    let cap_shorter_than_idle = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        ("HIRELING_SESSION_IDLE_SECS", "3600"),
        ("HIRELING_SESSION_ABSOLUTE_SECS", "1800"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ];
    let result = Settings::from_env(&env_with(&cap_shorter_than_idle));
    assert!(
        result.is_err(),
        "absolute shorter than idle is a misconfiguration"
    );

    let zero_idle_window = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        ("HIRELING_SESSION_IDLE_SECS", "0"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ];
    assert!(
        Settings::from_env(&env_with(&zero_idle_window)).is_err(),
        "a zero idle window is an error"
    );

    let valid_windows = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        ("HIRELING_SESSION_IDLE_SECS", "3600"),
        ("HIRELING_SESSION_ABSOLUTE_SECS", "7200"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
    ];
    let settings = Settings::from_env(&env_with(&valid_windows)).unwrap();
    assert_eq!(settings.auth.session_idle_secs, 3600);
    assert_eq!(settings.auth.session_absolute_secs, 7200);
}

#[test]
fn oidc_settings_parse_as_a_complete_group_with_derived_urls() {
    let env = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
        (
            "HIRELING_OIDC_ISSUER",
            "https://auth.flinntech.com/application/o/hireling/",
        ),
        ("HIRELING_OIDC_CLIENT_ID", "cid"),
        ("HIRELING_OIDC_CLIENT_SECRET", "sekrit"),
        ("HIRELING_BASE_URL", "https://hireling.flinntech.com"),
    ];
    let settings = Settings::from_env(&env_with(&env)).unwrap();
    let oidc = settings.auth.oidc.expect("all five present");
    assert_eq!(oidc.client_id, "cid");
    assert_eq!(
        oidc.redirect_uri(),
        "https://hireling.flinntech.com/api/auth/callback",
        "the redirect URI derives from the base URL"
    );
    assert_eq!(
        oidc.urls.token,
        "https://auth.flinntech.com/application/o/token/"
    );

    // A typo in one of the five must fail the boot, not silently disable login.
    let partial = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
        (
            "HIRELING_OIDC_ISSUER",
            "https://auth.flinntech.com/application/o/hireling/",
        ),
    ];
    assert!(Settings::from_env(&env_with(&partial)).is_err());
}

#[test]
fn release_validation_requires_the_oidc_group_and_cookie_key() {
    let without_oidc = Settings::from_env(&env_with(&auth_env())).unwrap();
    assert!(
        without_oidc.auth.validate_release().is_err(),
        "no OIDC configured"
    );

    let env = vec![
        ("HIRELING_ALLOWLIST", "sub-a"),
        ("HIRELING_GM_SUB", "sub-a"),
        (
            "HIRELING_COOKIE_KEY",
            "abababababababababababababababababababababababababababababababab",
        ),
        (
            "HIRELING_OIDC_ISSUER",
            "https://auth.flinntech.com/application/o/hireling/",
        ),
        ("HIRELING_OIDC_CLIENT_ID", "cid"),
        ("HIRELING_OIDC_CLIENT_SECRET", "sekrit"),
        ("HIRELING_BASE_URL", "https://hireling.flinntech.com"),
    ];
    let complete = Settings::from_env(&env_with(&env)).unwrap();
    assert!(
        complete.auth.validate_release().is_ok(),
        "complete configuration passes"
    );
}

/// An `OsString` that cannot be converted to a `String`.
#[cfg(unix)]
fn invalid_utf8() -> OsString {
    use std::os::unix::ffi::OsStringExt as _;

    OsString::from_vec(vec![0xff, 0xfe])
}

#[cfg(not(unix))]
fn invalid_utf8() -> OsString {
    use std::os::windows::ffi::OsStringExt as _;

    OsString::from_wide(&[0xd800])
}
