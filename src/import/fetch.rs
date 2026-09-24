//! Fetching the pinned release from upstream with bounded retry.
//!
//! One HTTP GET fetches the release metadata, one fetches the asset; the
//! asset's sha256 is verified against the digest the API itself reports,
//! so integrity does not trust the transport. Transient failures retry
//! with exponential backoff up to a bounded limit (FR-6); a digest
//! mismatch is corruption or tampering, not transience, and fails
//! immediately.

use anyhow::{Context as _, anyhow};
use serde_json::Value;
use sha2::{Digest as _, Sha256};
use std::time::Duration;

/// The upstream pack repository (Foundry VTT `pf2e` system packs).
pub const UPSTREAM_REPO: &str = "foundryvtt/pf2e";

/// Retry budget for fetches (FR-6: bounded).
const MAX_ATTEMPTS: u32 = 5;
/// Base for the exponential backoff; attempt n sleeps BASE << (n-1).
const BACKOFF_BASE: Duration = Duration::from_secs(1);
/// Jitter cap in milliseconds so concurrent operators do not sync retries.
const JITTER_CAP_MS: u32 = 250;

/// The release asset the importer needs, as the API describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAsset {
    /// Download URL for `json-assets.zip`.
    pub download_url: String,
    /// Expected sha256 hex, taken from the API's own `digest` field.
    pub sha256_hex: String,
}

/// Build the HTTP client the fetch path uses.
///
/// # Errors
///
/// Returns an error if the client cannot be built (TLS backend failure).
pub(crate) fn build_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .user_agent(concat!("hireling-importer/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(300))
        // Digests verify the bytes as served; transparent content-decoding
        // (gzip) would silently hash different bytes than the API's digest
        // covers. Disabling decoding is load-bearing for integrity.
        .no_gzip()
        .build()?)
}

/// Fetch the release metadata for `tag` and locate the importer's asset.
///
/// # Errors
///
/// Returns an error for unknown tags (no retry — 404 is permanent), for
/// exhausted retries on transient failures, and when the asset or its
/// digest is missing from the response.
pub(crate) async fn fetch_release_asset(
    client: &reqwest::Client,
    tag: &str,
) -> anyhow::Result<ReleaseAsset> {
    let url = format!("https://api.github.com/repos/{UPSTREAM_REPO}/releases/tags/{tag}");
    let response = with_retries(|attempt| {
        let request = client.get(&url);
        async move {
            match request.send().await {
                Ok(response) => status_outcome(response, "release metadata fetch", attempt),
                Err(err) => Attempt::Retryable(transport(err)),
            }
        }
    })
    .await?;
    let body = response
        .text()
        .await
        .context("release metadata body unreadable")?;
    let payload: Value =
        serde_json::from_str(&body).context("release metadata response was not JSON")?;
    let assets = payload
        .get("assets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("release metadata carried no assets array"))?;
    for asset in assets {
        if asset.get("name").and_then(Value::as_str) == Some("json-assets.zip") {
            let download_url = asset
                .get("browser_download_url")
                .and_then(Value::as_str)
                .ok_or_else(|| anyhow!("json-assets.zip asset carries no download url"))?
                .to_owned();
            let digest = asset.get("digest").and_then(Value::as_str).ok_or_else(|| {
                anyhow!("json-assets.zip asset carries no digest to verify against")
            })?;
            let sha256_hex = digest
                .strip_prefix("sha256:")
                .ok_or_else(|| anyhow!("asset digest `{digest}` is not a sha256 digest"))?
                .to_owned();
            return Ok(ReleaseAsset {
                download_url,
                sha256_hex,
            });
        }
    }
    Err(anyhow!(
        "release {tag} carries no `json-assets.zip` asset — the pack layout drifted"
    ))
}

/// Download bytes from `url` and verify their sha256 against `expected_hex`.
///
/// # Errors
///
/// Returns an error after exhausted retries on transient failures, and
/// immediately (no retry) on a digest mismatch.
pub(crate) async fn download_verified(
    client: &reqwest::Client,
    url: &str,
    expected_hex: &str,
) -> anyhow::Result<Vec<u8>> {
    // The whole transfer — send AND body read — sits inside the retry
    // loop: a truncated multi-megabyte download is exactly the transient
    // failure FR-6's backoff exists for.
    let bytes = with_retries(|attempt| async move {
        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(err) => return Attempt::Retryable(transport(err)),
        };
        let status = response.status();
        if !status.is_success() {
            return if is_retryable_status(status.as_u16()) {
                tracing::warn!(
                    status = status.as_u16(),
                    attempt = attempt + 1,
                    "asset fetch got a retryable status; backing off"
                );
                Attempt::Retryable(anyhow!("asset fetch returned {status}"))
            } else {
                Attempt::Fatal(anyhow!("asset fetch returned {status} (not retryable)"))
            };
        }
        match response.bytes().await {
            Ok(bytes) => Attempt::Done(bytes),
            Err(err) => {
                Attempt::Retryable(anyhow::anyhow!(err).context("asset download was interrupted"))
            }
        }
    })
    .await?;
    let actual = sha256_hex(&bytes);
    if actual != expected_hex {
        let preview = actual.get(..16).unwrap_or_default();
        return Err(anyhow!(
            "asset digest mismatch: expected sha256:{expected_hex}, got sha256:{preview}… \
             ({} bytes) — refusing to import (this is corruption or tampering, \
             not a transient failure)",
            bytes.len()
        ));
    }
    Ok(bytes.to_vec())
}

/// Fetch a small text file (used by the license archive step).
///
/// # Errors
///
/// Returns an error for unknown paths (no retry) or exhausted retries.
pub(crate) async fn fetch_text(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    with_retries(|attempt| async move {
        let response = match client.get(url).send().await {
            Ok(response) => response,
            Err(err) => return Attempt::Retryable(transport(err)),
        };
        match status_outcome(response, "text fetch", attempt) {
            Attempt::Done(response) => match response.text().await {
                Ok(text) => Attempt::Done(text),
                Err(err) => Attempt::Retryable(
                    anyhow::anyhow!(err).context("text response body unreadable"),
                ),
            },
            Attempt::Retryable(err) => Attempt::Retryable(err),
            Attempt::Fatal(err) => Attempt::Fatal(err),
        }
    })
    .await
}

/// Wrap a transport error as a retryable failure.
fn transport(err: reqwest::Error) -> anyhow::Error {
    anyhow::anyhow!(err).context("transport failure")
}

/// Classify one completed response for the retry loop.
fn status_outcome(
    response: reqwest::Response,
    what: &str,
    attempt: usize,
) -> Attempt<reqwest::Response> {
    let status = response.status();
    if status.is_success() {
        return Attempt::Done(response);
    }
    if is_retryable_status(status.as_u16()) {
        tracing::warn!(
            status = status.as_u16(),
            attempt = attempt + 1,
            "fetch got a retryable status; backing off"
        );
        Attempt::Retryable(anyhow!("{what} returned {status}"))
    } else {
        Attempt::Fatal(anyhow!("{what} returned {status} (not retryable)"))
    }
}

/// One fetch attempt's outcome.
enum Attempt<T> {
    /// Success — stop retrying.
    Done(T),
    /// Transient — back off and try again (bounded).
    Retryable(anyhow::Error),
    /// Permanent — fail the whole fetch now.
    Fatal(anyhow::Error),
}

/// Retry `attempt` up to [`MAX_ATTEMPTS`] times with exponential backoff
/// and jitter. The closure owns the whole attempt — send, status, and
/// body read — so any transient leg of the fetch gets the same budget.
async fn with_retries<T, F, Fut>(attempt: F) -> anyhow::Result<T>
where
    F: Fn(usize) -> Fut,
    Fut: std::future::Future<Output = Attempt<T>>,
{
    let mut last_error: Option<anyhow::Error> = None;
    let budget = usize::try_from(MAX_ATTEMPTS).unwrap_or(usize::MAX);
    for i in 0..budget {
        if i > 0 {
            let backoff = BACKOFF_BASE.saturating_mul(1 << (i - 1));
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |since| since.subsec_nanos());
            let jitter = Duration::from_millis(u64::from(nanos % JITTER_CAP_MS));
            tokio::time::sleep(backoff.saturating_add(jitter)).await;
        }
        match attempt(i).await {
            Attempt::Done(value) => return Ok(value),
            Attempt::Retryable(err) => {
                tracing::warn!(attempt = i + 1, error = %err, "fetch failed; backing off");
                last_error = Some(err);
            }
            Attempt::Fatal(err) => return Err(err),
        }
    }
    Err(last_error.unwrap_or_else(|| anyhow!("fetch exhausted retries without a recorded failure")))
}

/// GitHub statuses worth retrying: rate limiting and server-side faults.
fn is_retryable_status(status: u16) -> bool {
    status == 429 || status >= 500
}

/// sha256 over `bytes`, lowercase hex.
#[must_use]
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_digest(&digest)
}

#[cfg(test)]
#[path = "tests/fetch.rs"]
mod tests;

/// Lowercase hex-encode arbitrary bytes.
#[must_use]
pub(crate) fn hex_digest(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        // write! to a String cannot fail.
        write!(hex, "{byte:02x}").ok();
    }
    hex
}
