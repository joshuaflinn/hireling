//! The Docker `HEALTHCHECK` probe (E11, spec FR-2, design D1) — the one new
//! code artifact in E11.
//!
//! A ~60-line std-only binary: connect to the app's loopback port, speak one
//! `GET /healthz`, read the status line, exit 0 on 200 and non-zero otherwise
//! with a one-line stderr reason. It shares no code with the app it watches
//! (D1: the simplest wedge-detector is one that cannot wedge with it) and it
//! is dead weight to the server binary — a second build target of the same
//! crate, compiled by `cargo build --release --bins` in the image's build
//! stage and shipped at `/usr/local/bin/healthcheck`.
//!
//! The endpoint it probes (`src/health.rs`, routed in `src/http.rs`) is
//! dependency-free by design, so this probe reports process liveness, not
//! database health — a green check with errors elsewhere means a dependency
//! problem, and the runbook's triage table pairs the two signals.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpStream};
use std::process::ExitCode;
use std::time::Duration;

/// The app port when `HIRELING_PORT` is unset — `src/config.rs` is the
/// source of truth for the default (3000); this must match it.
const DEFAULT_PORT: &str = "3000";

/// The one request the probe speaks. `Connection: close` asks the server to
/// end the response, so the probe can read the status line and hang up
/// instead of framing chunks.
const REQUEST: &str = "GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n";

/// Per-step I/O budget. The image's `HEALTHCHECK --timeout` is the outer cap
/// (5s, design D1); connect and read each get 2s so a wedged process fails
/// its probe in under the cap with room for one retry's worth of scheduling.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(2);
const IO_TIMEOUT: Duration = Duration::from_secs(2);

fn main() -> ExitCode {
    // No `std::process::exit` — the workspace denies `clippy::exit` outside
    // `main.rs`, and an `ExitCode` return carries the same signal (Docker
    // reads the process status either way).
    match probe() {
        Ok(()) => ExitCode::SUCCESS,
        Err(reason) => {
            eprintln!("healthcheck: {reason}");
            ExitCode::FAILURE
        }
    }
}

/// One probe pass: resolve the port, connect, write the request, read the
/// status line, judge it. `Ok` only on HTTP 200.
fn probe() -> Result<(), String> {
    // Unset or empty means the image default; a set-but-invalid port fails
    // the probe (and therefore marks the container unhealthy) rather than
    // silently probing something else.
    let port = std::env::var("HIRELING_PORT")
        .ok()
        .filter(|port| !port.is_empty())
        .unwrap_or_else(|| DEFAULT_PORT.to_owned());
    let addr: SocketAddr = format!("127.0.0.1:{port}")
        .parse()
        .map_err(|err| format!("HIRELING_PORT {port:?} is not a valid port: {err}"))?;

    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|err| format!("connect {addr}: {err}"))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|err| format!("set write timeout: {err}"))?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|err| format!("set read timeout: {err}"))?;

    stream
        .write_all(REQUEST.as_bytes())
        .map_err(|err| format!("write request: {err}"))?;

    let mut status_line = String::new();
    BufReader::new(stream)
        .read_line(&mut status_line)
        .map_err(|err| format!("read status line: {err}"))?;
    parse_status_line(&status_line)
}

/// Judge one HTTP status line (`HTTP/1.1 200 OK`). `Ok` only on 200.
///
/// Parsing goes through `split_once`/`starts_with` because the workspace
/// denies `clippy::indexing_slicing` and `string_slice` — a status line is
/// exactly where a byte-slice habit would ship a panic-on-odd-input probe.
fn parse_status_line(status_line: &str) -> Result<(), String> {
    let line = status_line.trim_end();
    let Some((version, rest)) = line.split_once(' ') else {
        return Err(format!("not an HTTP status line: {line:?}"));
    };
    if !version.starts_with("HTTP/") {
        return Err(format!("not an HTTP status line: {line:?}"));
    }
    // The reason phrase is optional in HTTP (`HTTP/1.1 200` is legal), so
    // the second space may not exist.
    let code = match rest.split_once(' ') {
        Some((code, _reason)) => code,
        None => rest,
    };
    if code == "200" {
        Ok(())
    } else {
        Err(format!("/healthz answered HTTP {code}"))
    }
}

#[cfg(test)]
#[path = "tests/healthcheck.rs"]
mod tests;
