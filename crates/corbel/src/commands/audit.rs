use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use corbel_core::audit::{SymbolCoverage, coverage_for_changed_symbol, read_events_checked};
use corbel_core::budget::TokenBudget;
use corbel_core::hash::hash_bytes;
use corbel_core::path::RepoRoot;
use corbel_core::query::{self, SymbolInfo};
use corbel_core::store::migrate::open_for_serve;
use rusqlite::OptionalExtension;

pub fn run(path: &Path, since: Option<&str>) -> anyhow::Result<()> {
    let root = RepoRoot::new(path)
        .with_context(|| format!("failed to open repository root at {}", path.display()))?;

    let db_path = root.as_path().join(".corbel").join("index.db");
    let conn = open_for_serve(&db_path)
        .with_context(|| format!("failed to open index database at {}", db_path.display()))?;

    let audit_log_path = root.as_path().join(".corbel").join("audit.jsonl");
    let outcome = read_events_checked(&audit_log_path)
        .with_context(|| format!("failed to read audit log at {}", audit_log_path.display()))?;

    if outcome.corrupted_lines > 0 {
        println!(
            "Warning: {} audit log line{} could not be parsed and were ignored. \
             Query counts below may undercount actual corbel usage — a corrupted \
             line looks identical to a missing one otherwise.\n",
            outcome.corrupted_lines,
            if outcome.corrupted_lines == 1 {
                ""
            } else {
                "s"
            }
        );
    }

    let mut events = outcome.events;
    if let Some(since) = since {
        let cutoff =
            parse_since(since).with_context(|| format!("invalid --since value \"{since}\""))?;
        events.retain(|event| event.ts >= cutoff);
    }

    let mut changed_ranges = git_diff_line_ranges(root.as_path())?;
    if changed_ranges.is_empty() {
        println!("No uncommitted changes (git diff against HEAD is empty).");
        return Ok(());
    }

    let changed_files: Vec<String> = changed_ranges.keys().cloned().collect();
    let mut stale = Vec::new();
    let mut not_indexed = Vec::new();
    for file in &changed_files {
        match classify_file_freshness(&conn, root.as_path(), file)? {
            FileFreshness::Fresh => {}
            FileFreshness::Stale => stale.push(file.clone()),
            FileFreshness::NotIndexed => not_indexed.push(file.clone()),
        }
    }

    if !not_indexed.is_empty() {
        println!(
            "Note: {} file{} changed but not yet indexed — nothing to check coverage against:",
            not_indexed.len(),
            if not_indexed.len() == 1 { "" } else { "s" }
        );
        for file in &not_indexed {
            println!("  - {file}");
        }
        println!();
        for file in &not_indexed {
            changed_ranges.remove(file);
        }
    }

    if !stale.is_empty() {
        println!(
            "Warning: the corbel index is out of sync with HEAD for {} file{} — its indexed \
             content matches neither HEAD nor a state audit can verify against the current diff:",
            stale.len(),
            if stale.len() == 1 { "" } else { "s" }
        );
        for file in &stale {
            println!("  - {file}");
        }
        println!(
            "`corbel audit` maps diff line numbers to indexed symbols using HEAD as the common \
             coordinate system; that only works when the index was built while the working tree \
             matched HEAD. Re-run `corbel index` right after a commit (before making new edits), \
             then re-run audit. Skipping coverage analysis for these file(s).\n"
        );
        for file in &stale {
            changed_ranges.remove(file);
        }
    }

    if changed_ranges.is_empty() {
        println!("No remaining changes to analyze after excluding the file(s) above.");
        return Ok(());
    }

    let mut changed_symbols: Vec<SymbolInfo> = Vec::new();
    for (file, ranges) in &changed_ranges {
        let symbols = query::symbols_in_file(&conn, file)?;
        changed_symbols.extend(symbols_touched_by_ranges(&symbols, ranges));
    }

    if changed_symbols.is_empty() {
        println!(
            "Uncommitted changes found, but they don't overlap any indexed symbol definition."
        );
        return Ok(());
    }

    changed_symbols.sort_by(|a, b| a.file.cmp(&b.file).then(a.line.cmp(&b.line)));

    if events.is_empty() {
        println!(
            "No corbel queries recorded in this window.\n\
             The agent did not use corbel for these changes — coverage cannot be assessed.\n"
        );
        println!("Changed symbols:");
        for symbol in &changed_symbols {
            println!("  {} ({}:{})", symbol.name, symbol.file, symbol.line);
        }
        return Ok(());
    }

    let mut coverages = Vec::with_capacity(changed_symbols.len());
    for symbol in &changed_symbols {
        let impact_results = query::impact(
            &conn,
            &symbol.name,
            Some(&symbol.file),
            TokenBudget::new(usize::MAX),
        )?;
        let affected = impact_results
            .into_iter()
            .find(|result| result.target.file == symbol.file && result.target.line == symbol.line)
            .map(|result| result.affected)
            .unwrap_or_default();

        coverages.push(coverage_for_changed_symbol(
            &symbol.name,
            &symbol.file,
            symbol.line,
            &affected,
            &events,
        ));
    }

    let unchecked = coverages.iter().filter(|c| !c.impact_checked).count();
    println!(
        "{} quer{} recorded, {} symbol{} changed, {} unchecked\n",
        events.len(),
        if events.len() == 1 { "y" } else { "ies" },
        changed_symbols.len(),
        if changed_symbols.len() == 1 { "" } else { "s" },
        unchecked
    );

    for coverage in &coverages {
        print_coverage(coverage);
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileFreshness {
    /// The indexed content hash matches the HEAD blob hash: the index was
    /// built from exactly the state `git diff HEAD`'s old side describes, so
    /// old-side hunk coordinates line up with indexed symbol lines.
    Fresh,
    /// The file has an indexed row, but its hash matches neither a
    /// retrievable HEAD blob nor (by construction, since it got here) the
    /// only other candidate would be re-deriving it from the working tree —
    /// which is exactly the coordinate system `audit` cannot safely use.
    /// Either the index was built from some third state (e.g. mid-edit,
    /// before those edits were committed), or the file doesn't exist at
    /// HEAD at all despite having an indexed row (added to the index but
    /// never committed). No coordinate system can be trusted here.
    Stale,
    /// No indexed row for this file at all — never indexed, or indexed then
    /// deleted from the index. There's nothing to compare against, and
    /// `symbols_in_file` will return no symbols for it regardless, so this
    /// is reported separately from `Stale` rather than as a coordinate risk.
    NotIndexed,
}

/// Classifies a changed file's indexing freshness against HEAD rather than
/// against the current working tree. `git diff HEAD`'s hunks are always
/// expressed relative to HEAD on one side (the "old" side) and the working
/// tree on the other (the "new" side); `audit` matches hunks to indexed
/// symbols using the *old* side, so what must line up with the index is
/// HEAD, not whatever is currently on disk. Comparing against the working
/// tree instead would flag every file with an uncommitted edit as stale
/// unconditionally, since editing a file necessarily changes its hash —
/// defeating the entire point of auditing uncommitted changes.
fn classify_file_freshness(
    conn: &rusqlite::Connection,
    root: &Path,
    file: &str,
) -> anyhow::Result<FileFreshness> {
    let indexed_hash: Option<String> = conn
        .query_row("SELECT hash FROM files WHERE path = ?1", [file], |row| {
            row.get(0)
        })
        .optional()?;

    let Some(indexed_hash) = indexed_hash else {
        return Ok(FileFreshness::NotIndexed);
    };

    match head_blob_hash(root, file)? {
        Some(head_hash) if head_hash == indexed_hash => Ok(FileFreshness::Fresh),
        _ => Ok(FileFreshness::Stale),
    }
}

/// Returns the content hash of `file` as it exists at `HEAD`, or `None` if
/// `git show HEAD:<file>` fails — most commonly because the file doesn't
/// exist at HEAD (it's new and, at most, staged). Hashed with the same
/// `hash_bytes` function and raw-byte input the indexer uses, so the result
/// is directly comparable to `files.hash`.
fn head_blob_hash(root: &Path, file: &str) -> anyhow::Result<Option<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["show", &format!("HEAD:{file}")])
        .output()
        .context("failed to run `git show` — is git installed and is this a git repository?")?;

    if !output.status.success() {
        return Ok(None);
    }
    Ok(Some(hash_bytes(&output.stdout).to_string()))
}

fn print_coverage(coverage: &SymbolCoverage) {
    println!("{} ({}:{})", coverage.name, coverage.file, coverage.line);

    if !coverage.impact_checked {
        println!("  impact() never called on this symbol — blast radius was never checked.");
    } else if coverage.affected_total == 0 {
        println!("  impact() called — no callers found, nothing to inspect.");
    } else {
        let pct = coverage.affected_inspected as f64 / coverage.affected_total as f64 * 100.0;
        println!(
            "  impact() called — {}/{} affected symbols inspected via get_symbol ({pct:.0}%)",
            coverage.affected_inspected, coverage.affected_total
        );
        if !coverage.uninspected.is_empty() {
            println!("  not inspected:");
            for (name, file, line) in &coverage.uninspected {
                println!("    - {name} ({file}:{line})");
            }
        }
    }
    println!();
}

fn parse_since(input: &str) -> anyhow::Result<i64> {
    if let Ok(timestamp) = input.parse::<i64>() {
        return Ok(timestamp);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time is after unix epoch")
        .as_secs() as i64;

    let (digits, unit) = input.split_at(input.len().saturating_sub(1));
    let amount: i64 = digits.parse().with_context(|| {
        format!("expected a number followed by s/m/h/d, or a unix timestamp, got \"{input}\"")
    })?;
    let seconds = match unit {
        "s" => amount,
        "m" => amount * 60,
        "h" => amount * 3600,
        "d" => amount * 86400,
        _ => anyhow::bail!("unsupported duration suffix \"{unit}\" (use s, m, h, or d)"),
    };

    Ok(now - seconds)
}

fn git_diff_line_ranges(root: &Path) -> anyhow::Result<HashMap<String, Vec<(u32, u32)>>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["diff", "--unified=0", "--no-color", "HEAD"])
        .output()
        .context("failed to run `git diff` — is git installed and is this a git repository?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git diff failed: {}", stderr.trim());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut ranges: HashMap<String, Vec<(u32, u32)>> = HashMap::new();
    let mut current_file: Option<String> = None;

    for line in text.lines() {
        if let Some(path) = line.strip_prefix("+++ ") {
            current_file = parse_diff_path(path);
            // Register the file even if every hunk below turns out to
            // contribute no old-side range (e.g. a brand-new file, where
            // every hunk's old side is empty — see `parse_hunk_old_range`).
            // Without this, such files never appear as a key here at all,
            // making them invisible to the freshness check below instead of
            // being classified and reported as `NotIndexed`/`Stale`.
            if let Some(file) = &current_file {
                ranges.entry(file.clone()).or_default();
            }
            continue;
        }
        if line.starts_with("--- ") {
            continue;
        }
        if let Some(hunk) = line.strip_prefix("@@ ") {
            let Some(file) = current_file.as_ref() else {
                continue;
            };
            if let Some((start, count)) = parse_hunk_old_range(hunk) {
                if count > 0 {
                    ranges
                        .entry(file.clone())
                        .or_default()
                        .push((start, start + count - 1));
                } else if start > 0 {
                    // Pure addition (nothing removed on the old side): `start`
                    // is the old-file line after which new content was
                    // inserted, per unified-diff convention. Treat it as a
                    // single-point range so it's attributed to whichever
                    // symbol's approximated body contains that line — i.e.
                    // an insertion inside an existing symbol's body is
                    // attributed to that symbol. `start == 0` means the
                    // insertion happened before the first line of the file
                    // (or the file is new), which cannot be inside any
                    // symbol's body, so it's intentionally not recorded.
                    ranges.entry(file.clone()).or_default().push((start, start));
                }
            }
        }
    }

    Ok(ranges)
}

fn parse_diff_path(raw: &str) -> Option<String> {
    let raw = raw.split('\t').next().unwrap_or(raw).trim();
    if raw == "/dev/null" {
        return None;
    }
    raw.strip_prefix("b/").map(str::to_string)
}

/// Parses the old-file (`-`) side of a hunk header, e.g. `-3,4 +5,6 @@` ->
/// `(3, 4)`, or `-3 +5 @@` -> `(3, 1)` (an omitted count means 1). `audit`
/// matches hunks against indexed symbols using this side rather than the
/// new-file (`+`) side: indexed symbol lines come from the last `corbel
/// index` run, which — when `FileFreshness::Fresh` — reflects HEAD, the same
/// version `git diff`'s old side describes. Using the new side here would
/// compare post-edit line numbers against pre-edit symbol positions.
fn parse_hunk_old_range(hunk: &str) -> Option<(u32, u32)> {
    let minus_part = hunk.strip_prefix('-')?;
    let range_part = minus_part.split(' ').next()?;
    let mut pieces = range_part.splitn(2, ',');
    let start: u32 = pieces.next()?.trim().parse().ok()?;
    let count: u32 = match pieces.next() {
        Some(count) => count.trim().parse().ok()?,
        None => 1,
    };
    Some((start, count))
}

/// Approximates which symbols a set of changed line ranges touch. The
/// schema doesn't record where a symbol's body ends, so this treats each
/// symbol as spanning from its own definition line up to (but not
/// including) the next symbol's definition line — sorted by `line`
/// ascending. `symbols` is sorted defensively here (rather than trusting
/// the caller) even though `query::symbols_in_file`'s SQL already returns
/// rows `ORDER BY line`; this keeps the function correct on its own terms
/// if that query ever changes or a different caller feeds it unsorted data.
///
/// Known limitation: the *last* symbol in a file has no following symbol to
/// bound it, so its approximated body extends to end-of-file (line
/// `u32::MAX`). Appending a brand-new top-level symbol after it produces a
/// diff hunk in that same trailing region, so it is reported as a change to
/// the last *existing* symbol rather than recognized as a new one. A
/// precise fix requires the indexer to record each symbol's end line, which
/// it does not today — this is a known false-positive source, not a bug to
/// silently paper over. See README.md's "audit's known limitations" section.
fn symbols_touched_by_ranges(symbols: &[SymbolInfo], ranges: &[(u32, u32)]) -> Vec<SymbolInfo> {
    let mut sorted: Vec<&SymbolInfo> = symbols.iter().collect();
    sorted.sort_by_key(|s| s.line);

    let mut touched = Vec::new();
    for (index, symbol) in sorted.iter().enumerate() {
        let end = sorted
            .get(index + 1)
            .map(|next| next.line.saturating_sub(1))
            .unwrap_or(u32::MAX);
        let start = symbol.line;
        if ranges
            .iter()
            .any(|&(hunk_start, hunk_end)| start <= hunk_end && end >= hunk_start)
        {
            touched.push((*symbol).clone());
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .expect("git command runs");
        assert!(status.success(), "git {args:?} failed");
    }

    fn init_repo_with_commit(root: &Path, file: &str, content: &[u8]) {
        git(root, &["init", "-q"]);
        git(root, &["config", "user.email", "test@example.com"]);
        git(root, &["config", "user.name", "Test"]);
        fs::write(root.join(file), content).unwrap();
        git(root, &["add", file]);
        git(root, &["commit", "-q", "-m", "init"]);
    }

    #[test]
    fn parse_since_accepts_raw_unix_timestamp() {
        assert_eq!(parse_since("1700000000").unwrap(), 1700000000);
    }

    #[test]
    fn parse_since_rejects_unknown_suffix() {
        assert!(parse_since("5x").is_err());
    }

    #[test]
    fn parse_hunk_old_range_reads_start_and_count() {
        assert_eq!(parse_hunk_old_range("-1,2 +3,4 @@"), Some((1, 2)));
        assert_eq!(parse_hunk_old_range("-1 +5 @@"), Some((1, 1)));
    }

    #[test]
    fn parse_hunk_old_range_reads_pure_addition_as_zero_count() {
        assert_eq!(parse_hunk_old_range("-5,0 +6,3 @@"), Some((5, 0)));
    }

    #[test]
    fn parse_diff_path_strips_b_prefix_and_skips_dev_null() {
        assert_eq!(
            parse_diff_path("b/src/lib.rs"),
            Some("src/lib.rs".to_string())
        );
        assert_eq!(parse_diff_path("/dev/null"), None);
    }

    fn symbol(name: &str, line: u32) -> SymbolInfo {
        SymbolInfo {
            name: name.to_string(),
            file: "a.rs".to_string(),
            line,
            kind: "function".to_string(),
            signature: None,
            is_public: true,
        }
    }

    #[test]
    fn symbols_touched_by_ranges_matches_overlapping_symbol_only() {
        let symbols = vec![symbol("a", 1), symbol("b", 10), symbol("c", 20)];
        let touched = symbols_touched_by_ranges(&symbols, &[(10, 12)]);
        assert_eq!(touched, vec![symbol("b", 10)]);
    }

    #[test]
    fn symbols_touched_by_ranges_covers_last_symbol_to_end_of_file() {
        let symbols = vec![symbol("a", 1), symbol("b", 10)];
        let touched = symbols_touched_by_ranges(&symbols, &[(1000, 1000)]);
        assert_eq!(touched, vec![symbol("b", 10)]);
    }

    #[test]
    fn symbols_touched_by_ranges_sorts_unsorted_input_by_line() {
        let symbols = vec![symbol("c", 20), symbol("a", 1), symbol("b", 10)];
        let touched = symbols_touched_by_ranges(&symbols, &[(10, 12)]);
        assert_eq!(touched, vec![symbol("b", 10)]);
    }

    #[test]
    fn freshness_is_fresh_when_indexed_hash_matches_head_blob() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path(), "a.rs", b"fn a() {}\n");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        corbel_core::store::schema::create_schema(&conn).unwrap();
        let head_hash = hash_bytes(b"fn a() {}\n").to_string();
        conn.execute(
            "INSERT INTO files (path, lang, hash, indexed_at) VALUES ('a.rs', 'rs', ?1, 0)",
            [head_hash],
        )
        .unwrap();

        // Edit the working tree without touching the index or HEAD.
        fs::write(dir.path().join("a.rs"), b"fn a() { /* edited */ }\n").unwrap();

        let freshness = classify_file_freshness(&conn, dir.path(), "a.rs").unwrap();
        assert_eq!(freshness, FileFreshness::Fresh);
    }

    #[test]
    fn freshness_is_stale_when_indexed_hash_matches_neither_head_nor_a_verifiable_state() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path(), "a.rs", b"fn a() {}\n");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        corbel_core::store::schema::create_schema(&conn).unwrap();
        // Indexed hash reflects neither HEAD nor any file on disk — e.g. the
        // index was built mid-edit, before that edit was committed.
        let mid_edit_hash = hash_bytes(b"fn a() { /* mid-edit state */ }\n").to_string();
        conn.execute(
            "INSERT INTO files (path, lang, hash, indexed_at) VALUES ('a.rs', 'rs', ?1, 0)",
            [mid_edit_hash],
        )
        .unwrap();

        let freshness = classify_file_freshness(&conn, dir.path(), "a.rs").unwrap();
        assert_eq!(freshness, FileFreshness::Stale);
    }

    #[test]
    fn freshness_is_stale_when_indexed_but_absent_from_head() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path(), "committed.rs", b"fn c() {}\n");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        corbel_core::store::schema::create_schema(&conn).unwrap();
        // "new.rs" was indexed (e.g. via an uncommitted `corbel index` run)
        // but was never committed, so it doesn't exist at HEAD.
        let some_hash = hash_bytes(b"fn n() {}\n").to_string();
        conn.execute(
            "INSERT INTO files (path, lang, hash, indexed_at) VALUES ('new.rs', 'rs', ?1, 0)",
            [some_hash],
        )
        .unwrap();

        let freshness = classify_file_freshness(&conn, dir.path(), "new.rs").unwrap();
        assert_eq!(freshness, FileFreshness::Stale);
    }

    #[test]
    fn freshness_is_not_indexed_when_no_row_exists_regardless_of_git_state() {
        let dir = tempfile::tempdir().unwrap();
        init_repo_with_commit(dir.path(), "a.rs", b"fn a() {}\n");

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        corbel_core::store::schema::create_schema(&conn).unwrap();

        let freshness = classify_file_freshness(&conn, dir.path(), "a.rs").unwrap();
        assert_eq!(freshness, FileFreshness::NotIndexed);
    }
}
