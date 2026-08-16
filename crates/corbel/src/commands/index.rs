use std::fs;
use std::path::Path;

use anyhow::Context;
use corbel_core::index::{IndexStats, index_repo};
use corbel_core::path::RepoRoot;
use corbel_core::store::migrate::open_connection;
use corbel_lang::registry::LanguageRegistry;

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
    println!();

    let resolution = &stats.resolution;
    let internal_resolved = resolution.same_file + resolution.scoped + resolution.global_unique;
    let internal_total = internal_resolved + resolution.unresolved;

    if internal_total == 0 {
        println!("Internal calls: 0 resolved / 0 total (n/a)");
    } else {
        let pct = internal_resolved as f64 / internal_total as f64 * 100.0;
        println!(
            "Internal calls: {internal_resolved} resolved / {internal_total} total ({pct:.1}%)"
        );
    }
    println!(
        "  same-file: {}, scoped: {}, global-unique: {}",
        resolution.same_file, resolution.scoped, resolution.global_unique
    );
    println!("  unresolved (ambiguous): {}", resolution.unresolved);
    println!(
        "External calls: {} (std, crates, dynamic dispatch)",
        resolution.external
    );

    if resolution.unresolved > 0 {
        println!(
            "Note: {} internal call(s) could not be resolved because multiple definitions share the same name.",
            resolution.unresolved
        );
    }
}
