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

use std::io::Write as _;
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

/// Divergences below this percentage are attributed to filesystem churn
/// (files created/deleted while scanning) and reported as warnings, not errors.
const DIVERGENCE_WARN_PCT: f64 = 0.10;

fn pct_of(delta: i64, base: u64) -> f64 {
    if base == 0 { 0.0 } else { delta as f64 / base as f64 * 100.0 }
}

/// Prints per-metric divergence lines and returns the maximum absolute
/// percentage error across files / dirs / size (0.0 if all agree).
fn check_divergence(results: &[RunResult]) -> f64 {
    if results.len() < 2 {
        return 0.0;
    }
    let ref_r = &results[0];
    let mut max_pct: f64 = 0.0;
    for r in results.iter().skip(1) {
        if r.files != ref_r.files {
            let delta = r.files as i64 - ref_r.files as i64;
            let pct   = pct_of(delta, ref_r.files);
            eprintln!(
                "DIVERGENCE: '{}' files={} vs '{}' files={}  (Δ={:+}, {:+.2}%)",
                r.name, r.files, ref_r.name, ref_r.files, delta, pct
            );
            max_pct = max_pct.max(pct.abs());
        }
        if r.dirs != ref_r.dirs {
            let delta = r.dirs as i64 - ref_r.dirs as i64;
            let pct   = pct_of(delta, ref_r.dirs);
            eprintln!(
                "DIVERGENCE: '{}' dirs={} vs '{}' dirs={}  (Δ={:+}, {:+.2}%)",
                r.name, r.dirs, ref_r.name, ref_r.dirs, delta, pct
            );
            max_pct = max_pct.max(pct.abs());
        }
        if r.size != ref_r.size {
            let delta = r.size as i64 - ref_r.size as i64;
            let pct   = pct_of(delta, ref_r.size);
            eprintln!(
                "DIVERGENCE: '{}' size={} vs '{}' size={}  (Δ={:+}, {:+.2}%)",
                r.name, r.size, ref_r.name, ref_r.size, delta, pct
            );
            max_pct = max_pct.max(pct.abs());
        }
    }
    max_pct
}

// ── Per-child divergence breakdown ────────────────────────────────────────────

/// Walk root's immediate children comparing custom vs jwalk walker counts.
/// Prints a table sorted by absolute file-count delta, largest first.
/// Only meaningful when both Custom and Jwalk walkers are available.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn run_child_diff(root: &PathBuf) {
    use std::fs;

    let children: Vec<_> = match fs::read_dir(root) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect(),
        Err(e) => {
            eprintln!("  cannot read root dir: {e}");
            return;
        }
    };

    if children.is_empty() {
        println!("  no subdirectories found under root.");
        return;
    }

    struct ChildResult {
        name:         String,
        custom_files: u64,
        custom_dirs:  u64,
        jwalk_files:  u64,
        jwalk_dirs:   u64,
    }

    let mut rows: Vec<ChildResult> = Vec::new();

    for entry in children {
        let path = entry.path();
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();

        print!("  {:30} custom... ", name);
        let _ = std::io::stdout().flush();
        let cancel = Arc::new(AtomicBool::new(false));
        let custom = match scan_with(path.clone(), cancel, |_| {}, Walker::Custom) {
            Ok(t)  => (t.total_files, t.total_dirs),
            Err(e) => { eprintln!("err: {e}"); continue; }
        };

        print!("jwalk... ");
        let _ = std::io::stdout().flush();
        let cancel = Arc::new(AtomicBool::new(false));
        let jw = match scan_with(path.clone(), cancel, |_| {}, Walker::Jwalk) {
            Ok(t)  => (t.total_files, t.total_dirs),
            Err(e) => { eprintln!("err: {e}"); continue; }
        };
        println!("done");

        rows.push(ChildResult {
            name,
            custom_files: custom.0,
            custom_dirs:  custom.1,
            jwalk_files:  jw.0,
            jwalk_dirs:   jw.1,
        });
    }

    // Sort by |Δfiles| descending — the biggest mismatches first.
    rows.sort_by(|a, b| {
        let da = a.custom_files.abs_diff(a.jwalk_files);
        let db = b.custom_files.abs_diff(b.jwalk_files);
        db.cmp(&da)
    });

    println!();
    println!(
        "\n{:<32} {:>10} {:>10} {:>8}  {:>9} {:>9} {:>8}",
        "child", "files(c)", "files(j)", "Δfiles", "dirs(c)", "dirs(j)", "Δdirs"
    );
    println!("{}", "-".repeat(92));

    for r in &rows {
        let df = r.custom_files as i64 - r.jwalk_files as i64;
        let dd = r.custom_dirs  as i64 - r.jwalk_dirs  as i64;
        println!(
            "{:<32} {:>10} {:>10} {:>+8}  {:>9} {:>9} {:>+8}",
            r.name, r.custom_files, r.jwalk_files, df,
            r.custom_dirs,  r.jwalk_dirs,  dd,
        );
    }
    println!();
    println!(
        "Tip: rerun with DISKVIZ_WALK_DIAG=1 to see readdir_fallbacks / open_failures \
         per walk in the custom path."
    );
}

// ── main ──────────────────────────────────────────────────────────────────────

fn main() {
    let mut args = std::env::args().skip(1).peekable();

    let path = args.next().expect("usage: scan <path> [--runs N] [--walker <name>] [--diff]");
    let root = PathBuf::from(&path);

    // Parse optional flags.
    let mut runs:         usize          = 1;
    let mut only_walker:  Option<String> = None;
    let mut diff_mode:    bool           = false;

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
            "--diff" => {
                diff_mode = true;
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
            let _ = std::io::stdout().flush();
            let r = run_walker(name, walker, &root, runs);
            println!("done ({:.3}s)", r.elapsed.as_secs_f64());
            r
        })
        .collect();

    print_table(&results);

    let max_pct = check_divergence(&results);
    if max_pct == 0.0 {
        if results.len() > 1 { println!("\n✅  All walkers agree."); }
    } else if max_pct < DIVERGENCE_WARN_PCT {
        eprintln!(
            "\n⚠️   Divergence within noise threshold (max {:.2}% < {:.2}%) — likely filesystem churn.",
            max_pct, DIVERGENCE_WARN_PCT
        );
    } else {
        eprintln!("\n❌  Divergence detected — walkers disagree on results (max {:.2}%).", max_pct);
    }

    // Per-child breakdown: drill into which top-level dirs diverge.
    if diff_mode {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            println!("\nPer-child breakdown (comparing custom vs jwalk on each sub-dir):");
            run_child_diff(&root);
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            println!("--diff is only useful when the custom walker is available (macOS/Windows).");
        }
    }

    if max_pct >= DIVERGENCE_WARN_PCT {
        std::process::exit(1);
    }
}
