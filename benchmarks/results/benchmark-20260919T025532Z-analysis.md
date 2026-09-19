# 1.0.0 benchmark comparison — 2026-09-19

Companion to `benchmark-20260919T025532Z.json` / `.md`. Run after the 1.0.0
maintenance-mode changes: `end_line` recorded in the index schema (schema
v4→v5), `corbel audit` switched from body-range approximation to real
`end_line` boundaries, and an optional `depth` parameter added to the
`impact` MCP tool. Compared against the prior baseline run
`benchmark-20260903T142000Z` (preserved unmodified — see
`benchmark-20260903T142000Z-analysis.md`).

Reproduce with:

```
cargo build --release -p corbel
python3 benchmarks/harness/run_benchmark.py
```

## Pre-run checks

- `verify_commit`: all three repos still match their pinned commit exactly
  (unchanged from baseline — no drift):
  chevrotain `221fff76526021a3abba1068130f5e8a343fbaaf`,
  hyperfine `f12f3d9f86f3643b3b7deace5e160b1f0f44d2b7`,
  itsdangerous `672971d66a2ef9f85151e53283113f33d642dabd`.
- Indexes were already at schema v5 by the time this run's timings were
  captured (the `.corbel/` directories had been rebuilt once, immediately
  after the schema migration, as part of manual verification of Part A) —
  `corbel_index_summary` for this run correctly reports 0 changed files for
  all three repos, since none of the source under test changed between that
  rebuild and this run. This does not affect accuracy: the symbols, callers,
  and callees `get_symbol`/`find`/`impact` return are unaffected by the new
  `end_line` column, which only `corbel audit` reads.
- Same conditions as the baseline run: T1 (callers) + T4 (definition) only,
  T2 (callees) excluded by default, `BENCHMARK_TOKEN_BUDGET = 1,000,000`.
- **Truncated cases: 0** — no precision/recall number below is a truncation
  artifact.
- Diffing this run's raw per-repo aggregate table rows against the 0.3.0
  baseline run's is not byte-for-byte empty: the query-time column (e.g.
  `0.052` vs `0.051`, `0.286` vs `0.291`) differs by ordinary run-to-run
  wall-clock jitter, for corbel and for every comparison tool alike. Every
  precision/recall/F1/TP/FP/FN column, for every tool, in every repo, is
  identical between the two runs — verified directly, not inferred from the
  row diff being clean.

## 1. Overall precision / recall / F1 — before / after

Summed over all non-ambiguous T1+T4 entries across all three repos.

| Tool | Metric | Before (0.3.0, 142000Z) | After (1.0.0, 025532Z) | Δ |
| --- | --- | --- | --- | --- |
| **corbel** | TP / FP / FN | 358 / 147 / 150 | 358 / 147 / 150 | none |
| | Precision | 0.709 | 0.709 | none |
| | Recall | 0.705 | 0.705 | none |
| | F1 | 0.707 | 0.707 | none |
| grep | P / R / F1 | 0.472 / 0.640 / 0.543 | 0.472 / 0.640 / 0.543 | unchanged |
| ripgrep | P / R / F1 | 0.472 / 0.640 / 0.543 | 0.472 / 0.640 / 0.543 | unchanged |
| ripgrep+ctags | P / R / F1 | 0.617 / 0.844 / 0.713 | 0.617 / 0.844 / 0.713 | unchanged |

**No effect on accuracy.** This is expected, not incidental: Part A
(`end_line`) only touches the schema column read by `corbel audit`'s
`symbols_touched_by_ranges`, which this benchmark harness never calls, and
Part B (`impact`'s optional `depth`) is fully opt-in — every benchmark call
omits `depth`, which preserves the pre-1.0 default (walk to depth 10 or
budget exhaustion) exactly. corbel's own per-repo TP/FP/FN are byte-for-byte
identical between this run and the 0.3.0 run (chevrotain 178/27/39,
hyperfine 99/70/104, itsdangerous 81/50/7 in both), confirming neither
change altered any code path this harness exercises. (This is a corbel vs.
corbel comparison across time, not a comparison against `ripgrep+ctags` —
see the next paragraph for that.)

corbel's F1 (0.707) remains essentially tied with `ripgrep+ctags` (0.713)
and still behind it — **corbel does not win this comparison at 1.0.0**. The
gap is unchanged from 0.3.0 because nothing in this release touched name
resolution, call-site extraction, or the golden set. See
[benchmark-20260903T142000Z-analysis.md](benchmark-20260903T142000Z-analysis.md)
for the full failure-cause breakdown (name collisions, dynamic dispatch,
runtime prototype assembly, etc.) — none of it changed here and none of it
was in scope for 1.0.0 (see README's Known limitations and this release's
CHANGELOG entry).

## 2. Conclusion

Part A and Part B are index-schema and MCP-surface changes with no
resolution-logic component; the benchmark confirms they moved zero
precision/recall/F1 numbers, in either direction, across all three repos.
1.0.0 ships with the same accuracy profile as 0.3.0.
