//! Unit tests for the retry loop itself — pure, no HTTP.
//!
//! The wire-level behavior is exercised by live runs; here we pin the
//! contract that matters (FR-6): transient attempts retry with backoff up
//! to the bounded budget, and permanent failures stop immediately.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

#[tokio::test]
async fn transient_attempts_retry_then_succeed() {
    let attempts = AtomicUsize::new(0);
    let result: anyhow::Result<&'static str> = with_retries(|_| {
        let n = attempts.fetch_add(1, Ordering::SeqCst);
        async move {
            if n < 2 {
                Attempt::Retryable(anyhow!("flaky attempt {n}"))
            } else {
                Attempt::Done("payload")
            }
        }
    })
    .await;
    assert_eq!(
        result.expect("transient failures are eventually survived"),
        "payload"
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        3,
        "two retries then one success (FR-6: bounded, not infinite)"
    );
}

#[tokio::test]
async fn permanent_failures_stop_retrying_immediately() {
    let attempts = AtomicUsize::new(0);
    let result: anyhow::Result<()> = with_retries(|_| {
        attempts.fetch_add(1, Ordering::SeqCst);
        async { Attempt::Fatal(anyhow!("digest mismatch — corruption, not transience")) }
    })
    .await;
    assert!(result.is_err(), "a permanent failure surfaces as an error");
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        1,
        "no retry after a fatal outcome — corruption must not be retried into success theatre"
    );
}

#[tokio::test]
async fn exhausted_budget_returns_the_last_transient_error() {
    let attempts = AtomicUsize::new(0);
    let result: anyhow::Result<()> = with_retries(|_| {
        attempts.fetch_add(1, Ordering::SeqCst);
        async { Attempt::Retryable(anyhow!("still down")) }
    })
    .await;
    let err = result.expect_err("exhausted retries fail");
    assert!(
        format!("{err:#}").contains("still down"),
        "the operator sees the real last failure, not a generic message"
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        usize::try_from(MAX_ATTEMPTS).expect("attempt count fits usize"),
        "the budget is exactly MAX_ATTEMPTS"
    );
}
