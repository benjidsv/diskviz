//! Headless benchmark/correctness harness for the scanner.
//! Usage: cargo run --release --example scan -- <path>

use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("usage: scan <path>");
    let start = Instant::now();
    let tree = diskviz_lib::scanner::scan(PathBuf::from(&path), |_| {}).expect("scan failed");
    let elapsed = start.elapsed();

    println!("path:    {}", path);
    println!("size:    {} bytes", tree.total_size);
    println!("files:   {}", tree.total_files);
    println!("dirs:    {}", tree.total_dirs);
    println!("nodes:   {}", tree.nodes.len());
    println!("elapsed: {:?}", elapsed);
}
