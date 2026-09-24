//! Command-line parsing for the operator subcommands.
//!
//! Hand-rolled on purpose: two subcommands and a flag each do not justify a
//! parser dependency (Constitution Article V). The escape hatch stands — if
//! the CLI grows past ~3 subcommands, adopt `clap` with a written reason.

use std::ffi::OsString;

/// What the binary should do this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Default mode with no arguments: serve the app.
    Serve,
    /// Print help text and exit successfully.
    Usage(String),
    /// Import the rules corpus from a pinned upstream release.
    Import {
        /// The pinned release tag, e.g. `pf2e-8.5.1`.
        release: String,
    },
    /// Check the license gate and exit 0 (green) or 1 (red).
    LicenseVerdict,
    /// Archive the upstream pack license texts at a pinned release.
    LicenseArchive {
        /// The pinned release tag to archive from.
        release: String,
    },
}

/// Parse operator arguments (argv without argv[0]).
///
/// # Errors
///
/// Returns an error naming the problem for unknown subcommands, missing or
/// duplicated `--release` flags, and release tags that are not pinned
/// `pf2e-*` semantic tags.
pub fn parse(args: &[OsString]) -> anyhow::Result<Command> {
    let Some(first) = args.first() else {
        return Ok(Command::Serve);
    };
    let first = first.to_string_lossy();
    if first == "--help" || first == "-h" || first == "help" {
        return Ok(Command::Usage(USAGE_TEXT.to_owned()));
    }
    let rest = args.get(1..).unwrap_or(&[]);
    match first.as_ref() {
        "import" => Ok(Command::Import {
            release: release_arg(rest)?,
        }),
        "license-verdict" => flagless(rest, "license-verdict"),
        "license-archive" => Ok(Command::LicenseArchive {
            release: release_arg(rest)?,
        }),
        other => Err(anyhow::anyhow!(
            "unknown subcommand `{other}` — try `hireling --help`"
        )),
    }
}

fn flagless(rest: &[OsString], name: &str) -> anyhow::Result<Command> {
    if rest.is_empty() {
        Ok(Command::LicenseVerdict)
    } else {
        Err(anyhow::anyhow!("`{name}` takes no arguments"))
    }
}

fn release_arg(rest: &[OsString]) -> anyhow::Result<String> {
    if rest.len() != 2
        || rest.first().map(|arg| arg.to_string_lossy()).as_deref() != Some("--release")
    {
        return Err(anyhow::anyhow!("expected exactly `--release <tag>`"));
    }
    let tag = rest
        .get(1)
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_default();
    validate_release(&tag)?;
    Ok(tag)
}

/// Validate a pinned release tag: `pf2e-<major>.<minor>.<patch>`.
///
/// The upstream repo ships two systems from one repository; Starfinder tags
/// (`sf2e-*`) and moving names (`latest`) are not pf2e selectors and must
/// never import.
///
/// # Errors
///
/// Returns an error naming the rejected tag for anything that is not an
/// explicit `pf2e-N.N.N` tag.
pub fn validate_release(tag: &str) -> anyhow::Result<()> {
    let Some(number_part) = tag.strip_prefix("pf2e-") else {
        return Err(anyhow::anyhow!(
            "`{tag}` is not a pinned pf2e release tag — expected `pf2e-<major>.<minor>.<patch>` \
             (never `latest`, never `sf2e-*`)"
        ));
    };
    let segments: Vec<&str> = number_part.split('.').collect();
    if segments.len() != 3
        || segments
            .iter()
            .any(|s| s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(anyhow::anyhow!(
            "`{tag}` is not a pinned pf2e release tag — expected `pf2e-<major>.<minor>.<patch>` \
             (never `latest`, never `sf2e-*`)"
        ));
    }
    Ok(())
}

/// Parse a `pf2e-<major>.<minor>.<patch>` tag into its numeric triple.
///
/// Only called on tags that passed [`validate_release`].
#[must_use]
pub fn release_triple(tag: &str) -> (u64, u64, u64) {
    let number_part = tag.strip_prefix("pf2e-").unwrap_or(tag);
    let parsed: Vec<u64> = number_part
        .split('.')
        .filter_map(|segment| segment.parse().ok())
        .collect();
    match parsed.as_slice() {
        [major, minor, patch] => (*major, *minor, *patch),
        _ => (0, 0, 0),
    }
}

const USAGE_TEXT: &str = "\
hireling — party-linked Pathfinder 2e character tracker

default (no arguments): run the app server

operator subcommands:
  hireling import --release <tag>        import the rules corpus from a pinned
                                         upstream pack release (e.g. pf2e-8.5.1)
  hireling license-archive --release <tag>
                                         archive the upstream pack license texts
                                         into licenses/foundry-pf2e/
  hireling license-verdict               check the license gate; exit 0 green,
                                         1 red naming every violation
  hireling --help                        this text
";

#[cfg(test)]
#[path = "tests/args.rs"]
mod tests;
