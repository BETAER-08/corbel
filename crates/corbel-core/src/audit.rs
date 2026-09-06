use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::query::ImpactNode;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEvent {
    pub ts: i64,
    pub tool: String,
    pub name: String,
    pub file: String,
    pub line: u32,
}

impl AuditEvent {
    pub fn now(
        tool: impl Into<String>,
        name: impl Into<String>,
        file: impl Into<String>,
        line: u32,
    ) -> Self {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time is after unix epoch")
            .as_secs() as i64;
        AuditEvent {
            ts,
            tool: tool.into(),
            name: name.into(),
            file: file.into(),
            line,
        }
    }
}

pub fn append_event(log_path: &Path, event: &AuditEvent) -> Result<()> {
    if let Some(parent) = log_path.parent() {
        fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .map_err(|e| Error::io(log_path, e))?;
    let line = serde_json::to_string(event).expect("AuditEvent always serializes");
    writeln!(file, "{line}").map_err(|e| Error::io(log_path, e))?;
    Ok(())
}

pub fn read_events(log_path: &Path) -> Result<Vec<AuditEvent>> {
    Ok(read_events_checked(log_path)?.events)
}

/// Result of reading the audit log, including a count of lines that failed
/// to parse. A silently-dropped malformed line looks identical to "the tool
/// was never called" to every downstream consumer — the query count drops
/// and unchecked-symbol counts rise for a reason that has nothing to do with
/// agent behavior. Callers that report coverage to a human must surface
/// `corrupted_lines` rather than discard it, the same way indexing surfaces
/// skipped files instead of silently shrinking the symbol count.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadEventsOutcome {
    pub events: Vec<AuditEvent>,
    pub corrupted_lines: usize,
}

pub fn read_events_checked(log_path: &Path) -> Result<ReadEventsOutcome> {
    if !log_path.exists() {
        return Ok(ReadEventsOutcome::default());
    }
    let file = fs::File::open(log_path).map_err(|e| Error::io(log_path, e))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut corrupted_lines = 0usize;
    for line in reader.lines() {
        let line = line.map_err(|e| Error::io(log_path, e))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        match serde_json::from_str::<AuditEvent>(trimmed) {
            Ok(event) => events.push(event),
            Err(_) => corrupted_lines += 1,
        }
    }
    Ok(ReadEventsOutcome {
        events,
        corrupted_lines,
    })
}

fn was_queried(events: &[AuditEvent], tool: &str, name: &str, file: &str) -> bool {
    events
        .iter()
        .any(|e| e.tool == tool && e.name == name && e.file == file)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolCoverage {
    pub name: String,
    pub file: String,
    pub line: u32,
    pub impact_checked: bool,
    pub affected_total: usize,
    pub affected_inspected: usize,
    pub uninspected: Vec<(String, String, u32)>,
}

pub fn coverage_for_changed_symbol(
    name: &str,
    file: &str,
    line: u32,
    affected: &[ImpactNode],
    events: &[AuditEvent],
) -> SymbolCoverage {
    let impact_checked = was_queried(events, "impact", name, file);

    let mut affected_inspected = 0usize;
    let mut uninspected = Vec::new();
    for node in affected {
        if was_queried(events, "get_symbol", &node.name, &node.file) {
            affected_inspected += 1;
        } else {
            uninspected.push((node.name.clone(), node.file.clone(), node.line));
        }
    }

    SymbolCoverage {
        name: name.to_string(),
        file: file.to_string(),
        line,
        impact_checked,
        affected_total: affected.len(),
        affected_inspected,
        uninspected,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn node(name: &str, file: &str, line: u32) -> ImpactNode {
        ImpactNode {
            name: name.to_string(),
            file: file.to_string(),
            line,
            resolution: "same-file".to_string(),
            owner: None,
            lang: "rs".to_string(),
            depth: 1,
        }
    }

    #[test]
    fn append_and_read_round_trip() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join(".corbel").join("audit.jsonl");
        let event = AuditEvent::now("get_symbol", "widget", "src/lib.rs", 10);
        append_event(&log_path, &event).unwrap();

        let events = read_events(&log_path).unwrap();
        assert_eq!(events, vec![event]);
    }

    #[test]
    fn read_events_of_missing_file_is_empty() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");
        assert_eq!(read_events(&log_path).unwrap(), Vec::new());
    }

    #[test]
    fn read_events_checked_counts_corrupted_lines_instead_of_dropping_them() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");
        let event = AuditEvent::now("get_symbol", "widget", "src/lib.rs", 10);
        append_event(&log_path, &event).unwrap();
        std::fs::OpenOptions::new()
            .append(true)
            .open(&log_path)
            .unwrap()
            .write_all(b"{not valid json\n\n")
            .unwrap();

        let outcome = read_events_checked(&log_path).unwrap();
        assert_eq!(outcome.events, vec![event]);
        assert_eq!(outcome.corrupted_lines, 1);
    }

    #[test]
    fn read_events_checked_of_missing_file_reports_no_corruption() {
        let dir = tempdir().unwrap();
        let log_path = dir.path().join("audit.jsonl");
        let outcome = read_events_checked(&log_path).unwrap();
        assert_eq!(outcome.events, Vec::new());
        assert_eq!(outcome.corrupted_lines, 0);
    }

    #[test]
    fn coverage_reports_impact_never_called() {
        let affected = vec![node("caller_a", "a.rs", 5)];
        let coverage = coverage_for_changed_symbol("target", "a.rs", 1, &affected, &[]);
        assert!(!coverage.impact_checked);
        assert_eq!(coverage.affected_total, 1);
        assert_eq!(coverage.affected_inspected, 0);
    }

    #[test]
    fn coverage_matches_on_name_and_file_ignoring_line_drift() {
        let affected = vec![node("caller_a", "a.rs", 99)];
        let events = vec![
            AuditEvent::now("impact", "target", "a.rs", 1),
            AuditEvent::now("get_symbol", "caller_a", "a.rs", 5),
        ];
        let coverage = coverage_for_changed_symbol("target", "a.rs", 1, &affected, &events);
        assert!(coverage.impact_checked);
        assert_eq!(coverage.affected_inspected, 1);
        assert!(coverage.uninspected.is_empty());
    }

    #[test]
    fn coverage_lists_uninspected_affected_symbols() {
        let affected = vec![node("caller_a", "a.rs", 5), node("caller_b", "b.rs", 8)];
        let events = vec![
            AuditEvent::now("impact", "target", "a.rs", 1),
            AuditEvent::now("get_symbol", "caller_a", "a.rs", 5),
        ];
        let coverage = coverage_for_changed_symbol("target", "a.rs", 1, &affected, &events);
        assert_eq!(coverage.affected_inspected, 1);
        assert_eq!(
            coverage.uninspected,
            vec![("caller_b".to_string(), "b.rs".to_string(), 8)]
        );
    }
}
