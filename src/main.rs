//! Thin binary shell. Everything it does is: initialize tracing, call into the
//! library, and map the result to an exit code.
//!
//! Logic does not live here. `main.rs` is the one place allowed to call
//! `std::process::exit` (the `clippy::exit` lint denies it everywhere else),
//! which is exactly why it should be too small to hold a bug.

use std::process::ExitCode;

use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> ExitCode {
    // Tracing comes up before anything else so that even early failures land
    // somewhere. `RUST_LOG` controls the filter; default to `info`. Logs are
    // JSON on every environment so whatever scrapes them gets one shape.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    // argv must be UTF-8 (`std::env::args` panics otherwise): every input
    // this CLI accepts — subcommand names, `--release`, pinned `pf2e-N.N.N`
    // tags — is ASCII by contract, so non-UTF-8 argv has no legitimate use
    // and fails loudly at the boundary instead of being lossy-mangled into
    // a tag.
    //
    // nosemgrep: `rust.lang.security.args` fires on any binary that reads
    // its own arguments (CWE-807). Nothing here makes a security decision
    // from argv — it selects a subcommand; the security-relevant inputs
    // (the release tag grammar, the digest, the database URL) are each
    // validated downstream. There is no argv access that avoids this rule.
    let args: Vec<String> = std::env::args().skip(1).collect(); // nosemgrep
    match hireling::dispatch(&args).await {
        Ok(code) => code,
        Err(err) => {
            // Both channels on purpose: the structured record is for whatever
            // is scraping logs, the `{err:#}` line is for the human staring at
            // a terminal. `{err:?}` would print Rust type paths at them.
            tracing::error!(error = ?err, "fatal error");
            eprintln!("error: {err:#}");
            ExitCode::FAILURE
        }
    }
}
