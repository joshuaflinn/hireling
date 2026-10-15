//! The diskops shell (MOR-637): a free-space guard, the backend of
//! `just disk-guard`.
//!
//! All *decisions* live in the library (`src/lib.rs`) as pure functions —
//! threshold, parsing, the refuse/proceed outcome — so they are unit-tested
//! through the production path. This binary is only the shell: it reads the
//! environment, runs `df`, and maps the outcome to an exit code.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use diskops::{
    DEFAULT_FLOOR_GIB, FreeSpace, GIB, GuardOutcome, exhausted_walk_anchor, guard_outcome,
    parse_df, parse_floor_gib,
};

fn main() -> ExitCode {
    // No `std::process::exit` — the workspace denies `clippy::exit` outside
    // `main.rs`, and an `ExitCode` return carries the same signal.
    //
    // nosemgrep: `rust.lang.security.args` fires on any binary that reads
    // its own arguments (CWE-807). This binary only selects the guard
    // subcommand and a floor override from argv; it makes no security
    // decision from it. The floor is compared against a free-space
    // measurement regardless of argv. Same suppression the server binary
    // carries at `src/main.rs:35`.
    let args: Vec<String> = std::env::args().skip(1).collect(); // nosemgrep
    match dispatch(&args) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("diskops: {message}");
            ExitCode::FAILURE
        }
    }
}

fn dispatch(args: &[String]) -> Result<ExitCode, String> {
    let Some((subcommand, rest)) = args.split_first() else {
        return Err("no subcommand (expected guard)".to_owned());
    };
    match subcommand.as_str() {
        "guard" => guard(rest),
        other => Err(format!("unknown subcommand '{other}' (expected guard)")),
    }
}

/// The disk floor: refuse `ci-local` before the expensive legs when free
/// space cannot absorb one more build.
fn guard(args: &[String]) -> Result<ExitCode, String> {
    let mut floor_gib = DEFAULT_FLOOR_GIB;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(value) = arg.strip_prefix("--floor-gib=") {
            floor_gib = parse_floor_gib(value)?;
        } else if arg == "--floor-gib" {
            index += 1;
            let value = args
                .get(index)
                .ok_or_else(|| "guard: --floor-gib needs a number".to_owned())?;
            floor_gib = parse_floor_gib(value)?;
        } else {
            return Err(format!("guard: unknown option '{arg}'"));
        }
        index += 1;
    }

    let floor_bytes = floor_gib
        .checked_mul(GIB)
        .ok_or_else(|| format!("floor {floor_gib}G overflows a byte count"))?;

    // The filesystem that matters is the one holding the build output:
    // CARGO_TARGET_DIR when a seat points it elsewhere, else the worktree.
    // A CARGO_TARGET_DIR that does not yet exist (the detached-gate recipe
    // points at `../target-gate-<n>` before `just ci-local` creates it)
    // still names a filesystem — walk to the nearest existing ancestor.
    let target = std::env::var_os("CARGO_TARGET_DIR");
    let path: PathBuf = match target {
        Some(dir) => nearest_existing_ancestor(Path::new(&dir)),
        None => std::env::current_dir()
            .map_err(|error| format!("cannot read the current directory: {error}"))?,
    };

    let FreeSpace { available, mount } = df_avail(&path)?;

    match guard_outcome(available, floor_bytes, floor_gib, &mount) {
        GuardOutcome::Refuse { message } => {
            eprintln!("{message}");
            Ok(ExitCode::FAILURE)
        }
        GuardOutcome::Proceed { message } => {
            println!("{message}");
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `df` can only report a filesystem for a path that exists. A
/// `CARGO_TARGET_DIR` that does not yet exist would make `df` exit
/// non-zero, so walk up to the nearest existing ancestor — the filesystem
/// is the same either way.
fn nearest_existing_ancestor(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    loop {
        if current.exists() {
            return current;
        }
        if !current.pop() {
            // A relative path that runs out of components walks to the empty
            // path — the current directory's filesystem — not the root.
            // Round-2 F5: a single-component relative `CARGO_TARGET_DIR`
            // (`target-gate-123`) landed on `/` here and read the root
            // filesystem instead of the worktree's.
            return PathBuf::from(exhausted_walk_anchor(path.is_absolute()));
        }
    }
}

/// Run `df -P -B1 <path>` and hand the data row to the library parser.
fn df_avail(path: &Path) -> Result<FreeSpace, String> {
    let output = Command::new("df")
        .args(["-P", "-B1"])
        .arg(path)
        .output()
        .map_err(|error| format!("cannot run df: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "df failed on {}: {}",
            path.to_string_lossy(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let Ok(stdout) = String::from_utf8(output.stdout) else {
        return Err("df output is not valid UTF-8".to_owned());
    };
    parse_df(&stdout)
}
