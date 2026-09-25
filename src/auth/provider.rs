//! The HTTP side of the OIDC contract: the authorization-code exchange and
//! ID-token validation against the provider's JWKS.
//!
//! Hard rules from the captured house contract
//! (`specs/003-authentik-oidc/contracts/oidc.md`): client auth via
//! `client_secret_basic`, algorithms pinned to RS256 (forecloses the
//! alg-confusion class), `iss`/`aud`/`exp`/`nonce` all validated, keys cached
//! in memory and refetched on an unknown `kid`.

use std::sync::Arc;

use anyhow::Context as _;
use jsonwebtoken::Algorithm;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::Validation;
use serde::Deserialize;

use crate::auth::AuthState;
use crate::auth::oidc::IdTokenClaims;

/// The token-endpoint response fields the app consumes. Refresh tokens are
/// deliberately not requested or stored (data-model.md): session continuity
/// is Hireling's own sliding window.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
}

/// Exchange the authorization code for an ID token. The PKCE verifier and the
/// confidential-client credentials travel only to the token endpoint.
///
/// # Errors
///
/// Returns an error for any non-2xx response, transport failure, or a
/// response without an `id_token`.
pub async fn exchange_code(
    state: &AuthState,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> anyhow::Result<String> {
    let oidc = state.oidc.as_ref().context("OIDC is not configured")?;
    let form = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("code_verifier", code_verifier),
    ];
    let response = state
        .http
        .post(&oidc.urls.token)
        .basic_auth(&oidc.client_id, Some(&oidc.client_secret))
        .form(&form)
        .send()
        .await
        .context("token endpoint unreachable")?;

    let status = response.status();
    let body = response.text().await.context("token endpoint body read")?;
    if !status.is_success() {
        anyhow::bail!("token endpoint returned {status}");
    }
    let token: TokenResponse =
        serde_json::from_str(&body).context("token endpoint returned an unusable response")?;
    token
        .id_token
        .ok_or_else(|| anyhow::anyhow!("token endpoint returned no id_token"))
}

/// Get the decoding key for `kid`, from cache or a fresh JWKS fetch. A house
/// key rotation introduces a new `kid`; the first login to see it refetches.
///
/// # Errors
///
/// Returns an error when the JWKS fetch fails or no key matches `kid`.
pub async fn decoding_key(state: &AuthState, kid: &str) -> anyhow::Result<Arc<DecodingKey>> {
    {
        let cache = state.jwks.lock().await;
        if let Some(key) = cache.get(kid) {
            return Ok(Arc::clone(key));
        }
    }

    let oidc = state.oidc.as_ref().context("OIDC is not configured")?;
    let fetched: jsonwebtoken::jwk::JwkSet = state
        .http
        .get(&oidc.urls.jwks)
        .send()
        .await
        .context("JWKS endpoint unreachable")?
        .error_for_status()
        .context("JWKS endpoint returned an error status")?
        .json()
        .await
        .context("JWKS endpoint returned unusable JSON")?;

    let mut cache = state.jwks.lock().await;
    for jwk in &fetched.keys {
        if let Ok(key) = DecodingKey::from_jwk(jwk) {
            cache.insert(jwk.common.key_id.clone().unwrap_or_default(), Arc::new(key));
        }
    }
    cache
        .get(kid)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("no JWKS key matches kid {kid:?} after refetch"))
}

/// Validate an ID token end to end: RS256 signature against the JWKS,
/// issuer, audience, expiry (with small leeway), and the transaction nonce.
/// Returns the mapped claims on success.
///
/// # Errors
///
/// Returns an error for any failed check — the caller renders a generic
/// login-failed page; the reason stays in the log.
pub async fn validate_id_token(
    state: &AuthState,
    id_token: &str,
    expected_nonce: &str,
) -> anyhow::Result<IdTokenClaims> {
    let oidc = state.oidc.as_ref().context("OIDC is not configured")?;

    let header = jsonwebtoken::decode_header(id_token).context("ID token is not a readable JWT")?;
    if header.alg != Algorithm::RS256 {
        anyhow::bail!("ID token algorithm {:?} is not RS256", header.alg);
    }
    let kid = header
        .kid
        .ok_or_else(|| anyhow::anyhow!("ID token has no kid"))?;
    let key = decoding_key(state, &kid).await?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.leeway = 30;
    validation.set_issuer(&[oidc.issuer.as_str()]);
    validation.set_audience(&[oidc.client_id.as_str()]);

    let data = jsonwebtoken::decode::<IdTokenClaims>(id_token, &key, &validation)
        .context("ID token failed validation")?;
    let claims = data.claims;
    if claims.nonce.as_deref() != Some(expected_nonce) {
        anyhow::bail!("ID token nonce does not match the login transaction");
    }
    Ok(claims)
}
