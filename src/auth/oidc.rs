//! OIDC mechanics without I/O: token generation, PKCE, the signed
//! transaction cookie, claim mapping, provider URL derivation, and the
//! authorize redirect. The HTTP legs in `handlers`/`provider` call these;
//! everything here is pure and unit-tested against the captured house
//! Authentik contract (`specs/003-authentik-oidc/contracts/oidc.md`).

use std::time::Duration;

use anyhow::Context as _;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use chrono::DateTime;
use chrono::Utc;
use cookie::Cookie;
use cookie::CookieJar;
use cookie::Key;
use rand::TryRng as _;
use rand::rngs::SysRng;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;

/// Name of the short-lived signed cookie carrying the OIDC transaction state.
pub const TRANSACTION_COOKIE: &str = "__Host-hireling_oidc";
/// Name of the opaque session-lookup cookie.
pub const SESSION_COOKIE: &str = "__Host-hireling_session";
/// How long a login transaction (login → provider → callback) may take.
pub const TRANSACTION_TTL: Duration = Duration::from_secs(600);

/// Fill a buffer from the operating system's CSPRNG.
///
/// # Errors
///
/// Returns an error if the OS entropy source fails — never expected, always
/// fatal to the caller's operation (a session id we cannot make is a login we
/// cannot serve).
pub fn fill_random(buf: &mut [u8]) -> anyhow::Result<()> {
    SysRng
        .try_fill_bytes(buf)
        .map_err(|err| anyhow::Error::new(err).context("os random source failed"))
}

/// A fresh 256-bit token, base64url-encoded (43 chars).
///
/// # Errors
///
/// See [`fill_random`].
pub fn random_token() -> anyhow::Result<String> {
    let mut bytes = [0_u8; 32];
    fill_random(&mut bytes)?;
    Ok(B64URL.encode(bytes))
}

/// The PKCE S256 challenge for a verifier: BASE64URL(SHA256(verifier)).
#[must_use]
pub fn pkce_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    B64URL.encode(digest)
}

/// One login transaction: the three values minted at `/api/auth/login`,
/// carried through the provider round trip in the signed transaction cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transaction {
    pub state: String,
    pub nonce: String,
    pub code_verifier: String,
}

impl Transaction {
    /// Mint a fresh transaction.
    ///
    /// # Errors
    ///
    /// See [`fill_random`].
    pub fn generate() -> anyhow::Result<Self> {
        Ok(Self {
            state: random_token()?,
            nonce: random_token()?,
            code_verifier: random_token()?,
        })
    }

    /// Sign the transaction into a cookie value: BASE64URL(payload) with the
    /// HMAC-SHA256 digest prepended (the `cookie` crate's signed-jar layout).
    ///
    /// # Errors
    ///
    /// Returns an error if the payload cannot be serialized (a programming
    /// error, but reported rather than panicking).
    pub fn encode(&self, key: &Key, now: DateTime<Utc>) -> anyhow::Result<String> {
        let payload = TransactionPayload {
            state: self.state.clone(),
            nonce: self.nonce.clone(),
            code_verifier: self.code_verifier.clone(),
            exp: (now + chrono::Duration::from_std(TRANSACTION_TTL)?).timestamp(),
        };
        let json = serde_json::to_string(&payload).context("serialize transaction")?;

        let mut jar = CookieJar::new();
        jar.signed_mut(key).add((TRANSACTION_COOKIE, json));
        // `add` signs in place; the jar now holds the signed value.
        let signed = jar.get(TRANSACTION_COOKIE).map(Cookie::value);
        signed
            .map(str::to_owned)
            .ok_or_else(|| anyhow::anyhow!("signed transaction cookie missing from jar"))
    }

    /// Verify a cookie value against the key and the clock. Returns the
    /// transaction only when the signature is intact and `exp` is unexpired.
    #[must_use]
    pub fn decode(value: &str, key: &Key, now: DateTime<Utc>) -> Option<Self> {
        let mut jar = CookieJar::new();
        jar.add((TRANSACTION_COOKIE, value.to_owned()));
        let verified = jar.signed(key).get(TRANSACTION_COOKIE)?;
        let payload: TransactionPayload = serde_json::from_str(verified.value()).ok()?;
        if now.timestamp() >= payload.exp {
            return None;
        }
        Some(Self {
            state: payload.state,
            nonce: payload.nonce,
            code_verifier: payload.code_verifier,
        })
    }
}

#[derive(Serialize, Deserialize)]
struct TransactionPayload {
    state: String,
    nonce: String,
    code_verifier: String,
    exp: i64,
}

/// Collect the cookies of a `Cookie` header into a jar, so the signed-child
/// verification (which needs the key) sees all of them.
///
/// Malformed cookies are skipped, not fatal: an unparseable cookie is simply
/// one we cannot trust, and every caller treats "not found" as the denial
/// path.
/// path.
#[must_use]
pub fn jar_from_header(header_value: &str) -> CookieJar {
    let mut jar = CookieJar::new();
    for cookie in Cookie::split_parse(header_value).flatten() {
        jar.add(cookie.into_owned());
    }
    jar
}

/// The claims Hireling consumes from a validated ID token.
#[derive(Debug, Clone, Deserialize)]
pub struct IdTokenClaims {
    pub sub: String,
    pub nonce: Option<String>,
    pub preferred_username: Option<String>,
    pub name: Option<String>,
}

/// The account facts a successful login establishes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountIdentity {
    pub sub: String,
    pub username: String,
    pub display_name: String,
}

/// Map ID-token claims to account fields. Identity (`sub`) never depends on
/// profile claims; when a profile claim is absent, the seat name paired with
/// the `sub` in the allowlist config is the fallback, then the raw `sub`.
#[must_use]
pub fn map_claims(claims: &IdTokenClaims, seat_name: Option<&str>) -> AccountIdentity {
    let seat = seat_name.unwrap_or(&claims.sub);
    AccountIdentity {
        sub: claims.sub.clone(),
        username: claims
            .preferred_username
            .clone()
            .unwrap_or_else(|| seat.to_owned()),
        display_name: claims
            .name
            .clone()
            .or_else(|| claims.preferred_username.clone())
            .unwrap_or_else(|| seat.to_owned()),
    }
}

/// The three provider endpoints the app talks to, derived from the issuer per
/// the captured house contract (authorize and token are shared across
/// applications; JWKS is per-provider under the issuer path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderUrls {
    pub authorize: String,
    pub token: String,
    pub jwks: String,
}

/// Derive [`ProviderUrls`] from the issuer. The issuer must be an absolute
/// URL with a trailing path segment (e.g.
/// `https://auth.flinntech.com/application/o/hireling/`).
///
/// # Errors
///
/// Returns an error when the issuer is not an absolute HTTP(S) URL, so a
/// misconfigured deployment refuses to boot instead of building nonsense
/// redirect targets.
pub fn derive_provider_urls(issuer: &str) -> anyhow::Result<ProviderUrls> {
    let (scheme, rest) = issuer.split_once("://").ok_or_else(|| {
        anyhow::anyhow!("HIRELING_OIDC_ISSUER must be an absolute URL: {issuer:?}")
    })?;
    if scheme != "https" && scheme != "http" {
        anyhow::bail!("HIRELING_OIDC_ISSUER must be http(s): {issuer:?}");
    }
    let (authority, path) = rest
        .split_once('/')
        .ok_or_else(|| anyhow::anyhow!("HIRELING_OIDC_ISSUER must have a path: {issuer:?}"))?;
    if !path.ends_with('/') {
        anyhow::bail!("HIRELING_OIDC_ISSUER must end with '/': {issuer:?}");
    }
    Ok(ProviderUrls {
        authorize: format!("{scheme}://{authority}/application/o/authorize/"),
        token: format!("{scheme}://{authority}/application/o/token/"),
        jwks: format!("{scheme}://{authority}/{path}jwks/"),
    })
}

/// Build the provider redirect target for one transaction. Values are
/// percent-encoded; the verifier is sent only at the token endpoint.
#[must_use]
pub fn authorize_url(
    urls: &ProviderUrls,
    client_id: &str,
    redirect_uri: &str,
    transaction: &Transaction,
) -> String {
    let query = [
        ("response_type", "code"),
        ("scope", "openid profile"),
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("state", &transaction.state),
        ("nonce", &transaction.nonce),
        (
            "code_challenge",
            &pkce_challenge(&transaction.code_verifier),
        ),
        ("code_challenge_method", "S256"),
    ]
    .map(|(key, value)| format!("{key}={}", percent_encode(value)))
    .join("&");
    format!("{}?{query}", urls.authorize)
}

/// Percent-encode one query value. Unreserved characters pass through;
/// everything else becomes `%XX` so provider parsing cannot be surprised.
fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(*byte as char);
            }
            _ => {
                // Writing to a `String` never fails; `fmt::Write` carries the
                // `Result` for other sinks, so `.ok()` is the honest no-op.
                write!(out, "%{byte:02X}").ok();
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "tests/oidc.rs"]
mod tests;
