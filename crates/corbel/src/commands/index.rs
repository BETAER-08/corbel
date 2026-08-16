use std::fs;
use std::path::Path;

use anyhow::Context;
use corbel_core::index::{IndexStats, index_repo};
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_connection;
use corbel_lang::registry::LanguageRegistry;

const UNRESOLVED_WARNING_THRESHOLD: f64 = 50.0;

pub fn run(path: &Path, verbose: bool) -> anyhow::Result<()> {
    let root = RepoRoot::new(path)
        .with_context(|| format!("failed to open repository root at {}", path.display()))?;

    let corbel_dir = root.as_path().join(".corbel");
    fs::create_dir_all(&corbel_dir)
        .with_context(|| format!("failed to create {}", corbel_dir.display()))?;

    let db_path = corbel_dir.join("index.db");
    let conn = open_connection(&db_path)
        .with_context(|| format!("failed to open index database at {}", db_path.display()))?;

    let registry = LanguageRegistry::with_all_languages();

    let stats = index_repo(&root, &conn, &registry)?;

    if verbose {
        tracing::debug!(
            files_indexed = stats.files_indexed,
            files_skipped_unchanged = stats.files_skipped_unchanged,
            symbols_stored = stats.symbols_stored,
            imports_stored = stats.imports_stored,
            relationships_stored = stats.relationships_stored,
            references_skipped_no_caller = stats.references_skipped_no_caller,
            "indexing complete"
        );
    }

    let total_symbols: i64 =
        conn.query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))?;
    let total_relationships: i64 =
        conn.query_row("SELECT COUNT(*) FROM relationships", [], |row| row.get(0))?;

    print_summary(&stats, total_symbols, total_relationships);

    Ok(())
}

fn print_summary(stats: &IndexStats, total_symbols: i64, total_relationships: i64) {
    println!(
        "Indexed {} files ({} unchanged)",
        stats.files_indexed, stats.files_skipped_unchanged
    );
    println!("{total_symbols} symbols, {total_relationships} references");

    let resolution = &stats.resolution;
    let total =
        resolution.same_file + resolution.scoped + resolution.global_unique + resolution.unresolved;

    println!(
        "Resolution: {} same-file, {} scoped, {} global-unique, {} unresolved",
        resolution.same_file, resolution.scoped, resolution.global_unique, resolution.unresolved
    );

    if total == 0 {
        println!("Unresolved: 0/0 (n/a)");
        return;
    }

    let pct = resolution.unresolved as f64 / total as f64 * 100.0;
    println!(
        "Unresolved: {}/{} ({pct:.1}%)",
        resolution.unresolved, total
    );

    if pct > UNRESOLVED_WARNING_THRESHOLD {
        println!("Warning: over half of the detected call relationships remain unresolved.");
    }
}
