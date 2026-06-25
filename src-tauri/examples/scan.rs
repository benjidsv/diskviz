//! Multi-walker comparison harness for the diskviz scanner.
//!
//! Runs every walker available on the current platform over the same path and
//! prints a side-by-side results table. Any divergence in files / dirs / size
//! across walkers is flagged as a correctness failure.
//!
//! Usage:
//!   cargo run --release --example scan -- <path> [--runs N] [--walker <name>]
//!
//! Options:
//!   --runs N        Average timing over N runs (default: 1).
//!   --walker <name> Run only the named walker (custom | jwalk).
//!
//! Exit code is 1 if any divergence is detected.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use diskviz_lib::scanner::{scan_with, Walker};

// ── Walker catalogue ──────────────────────────────────────────────────────────

/// All walkers available on the current build target.
fn available_walkers() -> Vec<(&'static str, Walker)> {
    let mut w: Vec<(&'static str, Walker)> = Vec::new();

    // Platform fast-path (custom): always first so it anchors the reference
    // numbers when divergence is checked.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    w.push(("custom", Walker::Custom));

    // jwalk fallback: available everywhere.
    w.push(("jwalk", Walker::Jwalk));

    // Future: Walker::Mft will be inserted here when the MFT walker lands.

    w
}

// ── Result record ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
struct RunResult {
    name:    &'static str,
    files:   u64,
    dirs:    u64,
    size:    u64,
    nodes:   usize,
    elapsed: std::time::Duration,
}

fn run_walker(
    name:    &'static str,
    walker:  Walker,
    root:    &PathBuf,
    runs:    usize,
) -> RunResult {
    let mut total_elapsed = std::time::Duration::ZERO;
    let mut last_files = 0;
    let mut last_dirs  = 0;
    let mut last_size  = 0;
    let mut last_nodes = 0;

    for _ in 0..runs {
        let cancel = Arc::new(AtomicBool::new(false));
        let t0 = Instant::now();
        let tree = scan_with(root.clone(), cancel, |_| {}, walker).expect("scan failed");
        total_elapsed += t0.elapsed();
        last_files = tree.total_files;
        last_dirs  = tree.total_dirs;
        last_size  = tree.total_size;
        last_nodes = tree.nodes.len();
    }

    RunResult {
        name,
        files:   last_files,
        dirs:    last_dirs,
        size:    last_size,
        nodes:   last_nodes,
        elapsed: total_elapsed / runs as u32,
    }
}

// ── Table printing ────────────────────────────────────────────────────────────

fn print_table(results: &[RunResult]) {
    // Find the slowest run to compute speedups.
    let slowest_ns = results
        .iter()
        .map(|r| r.elapsed.as_nanos())
        .max()
        .unwrap_or(1)
        .max(1);

    // Header.
    println!(
        "\n{:<10} {:>12} {:>10} {:>16} {:>10} {:>10} {:>8}",
        "walker", "files", "dirs", "size(bytes)", "nodes", "elapsed", "speedup"
    );
    println!("{}", "-".repeat(82));

    for r in results {
        let speedup = slowest_ns as f64 / r.elapsed.as_nanos().max(1) as f64;
        println!(
            "{:<10} {:>12} {:>10} {:>16} {:>10} {:>10} {:>7.2}x",
            r.name,
            r.files,
            r.dirs,
            r.size,
            r.nodes,
            format!("{:.3}s", r.elapsed.as_secs_f64()),
            speedup,
        );
    }
}

// ── Divergence check ──────────────────────────────────────────────────────────

/// Returns true if any walker disagrees with the first result.
fn check_divergence(results: &[RunResult]) -> bool {
    if results.len() < 2 {
        return false;
    }
    let ref_r = &results[0];
    let mut any = false;
    for r in results.iter().skip(1) {
        if r.files != ref_r.files {
            eprintln!(
                "DIVERGENCE: '{}' files={} vs '{}' files={}",
                r.name, r.files, ref_r.name, ref_r.files
            );
            any = true;
        }
        if r.dirs != ref_r.dirs {
            eprintln!(
                "DIVERGENCE: '{}' dirs={} vs '{}' dirs={}",
                r.name, r.dirs, ref_r.name, ref_r.dirs
            );
            any = true;
        }
        if r.size != ref_r.size {
            eprintln!(
                "DIVERGENCE: '{}' size={} vs '{}' size={}",
                r.name, r.size, ref_r.name, ref_r.size
            );
            any = true;
        }
    }
    any
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let mut args = std::env::args().skip(1).peekable();

    let path = args.next().expect("usage: scan <path> [--runs N] [--walker <name>]");
    let root = PathBuf::from(&path);

    // Parse optional flags.
    let mut runs: usize = 1;
    let mut only_walker: Option<String> = None;

    while let Some(flag) = args.next() {
        match flag.as_str() {
            "--runs" => {
                runs = args
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1)
                    .max(1);
            }
            "--walker" => {
                only_walker = args.next();
            }
            _ => {}
        }
    }

    // Determine which walkers to run.
    let all = available_walkers();
    let walkers: Vec<(&'static str, Walker)> = if let Some(ref name) = only_walker {
        let found = all.iter().find(|(n, _)| n == name);
        match found {
            Some(&(n, w)) => vec![(n, w)],
            None => {
                eprintln!(
                    "Unknown walker '{}'. Available: {}",
                    name,
                    all.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
                );
                std::process::exit(1);
            }
        }
    } else {
        all
    };

    if walkers.is_empty() {
        eprintln!("No walkers available on this platform.");
        std::process::exit(1);
    }

    println!("Scanning: {path}");
    println!("Runs per walker: {runs}");

    let results: Vec<RunResult> = walkers
        .into_iter()
        .map(|(name, walker)| {
            print!("  running '{name}'... ");
            let r = run_walker(name, walker, &root, runs);
            println!("done ({:.3}s)", r.elapsed.as_secs_f64());
            r
        })
        .collect();

    print_table(&results);

    if check_divergence(&results) {
        eprintln!("\n❌  Divergence detected — walkers disagree on results.");
        std::process::exit(1);
    } else if results.len() > 1 {
        println!("\n✅  All walkers agree.");
    }
}
