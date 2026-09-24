//! Unit tests for CLI argument parsing.

use super::*;

fn args(items: &[&str]) -> Vec<String> {
    items.iter().copied().map(String::from).collect()
}

#[test]
fn no_arguments_means_serve() {
    assert_eq!(
        parse(&[]).expect("empty args parse"),
        Command::Serve,
        "no args must mean serve"
    );
}

#[test]
fn help_flags_print_usage() {
    for flag in ["--help", "-h", "help"] {
        let parsed = parse(&args(&[flag])).expect("help flag parses");
        assert!(
            matches!(parsed, Command::Usage(text) if text.contains("import")),
            "`{flag}` must yield usage text naming the subcommands"
        );
    }
}

#[test]
fn import_requires_a_release_flag() {
    let err = parse(&args(&["import"])).expect_err("missing --release must fail");
    assert!(
        err.to_string().contains("--release"),
        "error must name the expected flag, got: {err}"
    );
}

#[test]
fn import_takes_a_pinned_release() {
    let parsed = parse(&args(&["import", "--release", "pf2e-8.5.1"])).expect("valid import parses");
    assert_eq!(
        parsed,
        Command::Import {
            release: "pf2e-8.5.1".to_owned()
        },
        "import must carry the pinned release"
    );
}

#[test]
fn import_refuses_latest() {
    let err = parse(&args(&["import", "--release", "latest"])).expect_err("latest must be refused");
    assert!(
        err.to_string().contains("latest"),
        "refusal must name the moving target danger, got: {err}"
    );
}

#[test]
fn import_refuses_starfinder_tags() {
    let err = parse(&args(&["import", "--release", "sf2e-1.5.1"]))
        .expect_err("sf2e tags must be refused");
    assert!(
        err.to_string().contains("sf2e-1.5.1"),
        "refusal must name the rejected tag, got: {err}"
    );
}

#[test]
fn import_refuses_malformed_versions() {
    for tag in [
        "pf2e-8",
        "pf2e-8.5",
        "pf2e-8.5.1.1",
        "pf2e-eight.5.1",
        "pf2e-8.5.x",
        "pf2e-",
    ] {
        let parsed = parse(&args(&["import", "--release", tag]));
        assert!(parsed.is_err(), "`{tag}` must be refused");
    }
}

#[test]
fn unknown_subcommands_are_rejected() {
    let err = parse(&args(&["selfdestruct"])).expect_err("unknown subcommand must fail");
    assert!(
        err.to_string().contains("selfdestruct"),
        "error must name the unknown subcommand, got: {err}"
    );
}

#[test]
fn verdict_takes_no_arguments() {
    assert_eq!(
        parse(&args(&["license-verdict"])).expect("verdict parses"),
        Command::LicenseVerdict,
        "license-verdict alone must parse"
    );
    assert!(
        parse(&args(&["license-verdict", "extra"])).is_err(),
        "license-verdict must reject arguments"
    );
}

#[test]
fn release_triples_compare_numerically() {
    assert!(
        release_triple("pf2e-8.5.1") < release_triple("pf2e-10.0.0"),
        "10 must compare greater than 8 — lexicographic order would lie"
    );
    assert_eq!(
        release_triple("pf2e-8.5.1"),
        (8, 5, 1),
        "triple must split on dots"
    );
}
