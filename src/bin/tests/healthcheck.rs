//! Tests for [`super`].
//!
//! The status-line parse is the only decision the probe makes, so it is the
//! part unit-tested against canned responses (plan Task 1). Both sides of
//! the 200 boundary appear: a passing fixture and a failing fixture — a
//! one-sided suite would pass against a parse that accepted everything.

use super::parse_status_line;

#[test]
fn status_200_with_reason_is_accepted() {
    assert!(
        parse_status_line("HTTP/1.1 200 OK\r\n").is_ok(),
        "a normal 200 OK status line must pass the probe"
    );
}

#[test]
fn status_200_without_reason_is_accepted() {
    assert!(
        parse_status_line("HTTP/1.1 200\r\n").is_ok(),
        "an empty reason phrase is legal HTTP; the code alone decides"
    );
}

#[test]
fn status_500_is_rejected_and_named() {
    let reason = parse_status_line("HTTP/1.1 500 Internal Server Error\r\n").unwrap_err();

    assert!(
        reason.contains("500"),
        "the stderr reason must name the status code, got: {reason}"
    );
}

#[test]
fn status_404_is_rejected() {
    assert!(
        parse_status_line("HTTP/1.0 404 Not Found\r\n").is_err(),
        "any non-200 code must fail the probe"
    );
}

#[test]
fn garbage_line_is_rejected() {
    assert!(
        parse_status_line("garbage\r\n").is_err(),
        "a line that is not an HTTP status line must fail the probe"
    );
}

#[test]
fn non_http_version_token_is_rejected() {
    assert!(
        parse_status_line("NOTHTTP 200 OK\r\n").is_err(),
        "a 200-looking line without an HTTP/ version prefix must fail"
    );
}

#[test]
fn empty_response_is_rejected() {
    assert!(
        parse_status_line("").is_err(),
        "connection closed before the status line must fail the probe"
    );
}
