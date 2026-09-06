use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use corbel_core::audit::{SymbolCoverage, coverage_for_changed_symbol, read_events};
use corbel_core::budget::TokenBudget;
use corbel_core::path::RepoRoot;
use corbel_core::query::{self, SymbolInfo};
use corbel_core::store::migrate::open_for_serve;

pub fn run(path: &Path, since: Option<&str>) -> anyhow::Result<()> {
    let root = RepoRoot::new(path)
        .with_context(|| format!("failed to open repository root at {}", path.display()))?;

    let db_path = root.as_path().join(".corbel").join("index.db");
    let conn = open_for_serve(&db_path)
        .with_context(|| format!("failed to open index database at {}", db_path.display()))?;

    let audit_log_path = root.as_path().join(".corbel").join("audit.jsonl");
    let mut events = read_events(&audit_log_path)
        .with_context(|| format!("failed to read audit log at {}", audit_log_path.display()))?;

    if let Some(since) = since {
        let cutoff =
            parse_since(since).with_context(|| format!("invalid --since value \"{since}\""))?;
        events.retain(|event| event.ts >= cutoff);
    }

    let changed_ranges = git_diff_line_ranges(root.as_path())?;
    if changed_ranges.is_empty() {
        println!("No uncommitted changes (git diff against HEAD is empty).");
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
            continue;
        }
        if line.starts_with("--- ") {
            continue;
        }
        if let Some(hunk) = line.strip_prefix("@@ ") {
            let Some(file) = current_file.as_ref() else {
                continue;
            };
            if let Some((start, count)) = parse_hunk_new_range(hunk) {
                if count > 0 {
                    ranges
                        .entry(file.clone())
                        .or_default()
                        .push((start, start + count - 1));
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

fn parse_hunk_new_range(hunk: &str) -> Option<(u32, u32)> {
    let plus_part = hunk.split(" +").nth(1)?;
    let range_part = plus_part.split(" @@").next()?;
    let mut pieces = range_part.splitn(2, ',');
    let start: u32 = pieces.next()?.trim().parse().ok()?;
    let count: u32 = match pieces.next() {
        Some(count) => count.trim().parse().ok()?,
        None => 1,
    };
    Some((start, count))
}

fn symbols_touched_by_ranges(symbols: &[SymbolInfo], ranges: &[(u32, u32)]) -> Vec<SymbolInfo> {
    let mut touched = Vec::new();
    for (index, symbol) in symbols.iter().enumerate() {
        let end = symbols
            .get(index + 1)
            .map(|next| next.line.saturating_sub(1))
            .unwrap_or(u32::MAX);
        let start = symbol.line;
        if ranges
            .iter()
            .any(|&(hunk_start, hunk_end)| start <= hunk_end && end >= hunk_start)
        {
            touched.push(symbol.clone());
        }
    }
    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_since_accepts_raw_unix_timestamp() {
        assert_eq!(parse_since("1700000000").unwrap(), 1700000000);
    }

    #[test]
    fn parse_since_rejects_unknown_suffix() {
        assert!(parse_since("5x").is_err());
    }

    #[test]
    fn parse_hunk_new_range_reads_start_and_count() {
        assert_eq!(parse_hunk_new_range("-1,2 +3,4 @@"), Some((3, 4)));
        assert_eq!(parse_hunk_new_range("-1 +5 @@"), Some((5, 1)));
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
}
