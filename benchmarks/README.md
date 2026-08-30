# Benchmarks

This directory holds corbel's comparison benchmark against grep, ripgrep, and
ctags. It replaces the amdb-inherited harness in its entirety; nothing here
reuses that code or its assumptions.

`benchmarks/repos/` and `benchmarks/results/` are ignored by git. The harness
clones external repositories into `benchmarks/repos/` and writes measurements
to `benchmarks/results/`. Never commit the contents of either directory.

## Layout

- `golden/*.json` — version-controlled golden sets, one per repository. Each
  entry records the exact symbol under test, its category (`simple`,
  `multi_hop`, `overload_ambiguous_name`, `dynamic_dispatch`,
  `qualified_path_call`, `external_boundary`), the manually verified expected
  callers/callees/definition, who verified it, and how (`verification_note`
  per entry, `verification_method` per file). corbel was never run to produce
  any of this data — it was derived by reading source with the Read tool and
  cross-checking with ripgrep-assisted line enumeration, to avoid the circular
  argument of grading corbel against its own output.
- `harness/` — the measurement harness, pure Python 3 standard library, no
  third-party dependencies.
  - `enclosing.py` — regex/indentation-based "which function contains this
    line" resolver, used by the grep and ripgrep adapters.
  - `tool_adapters.py` — one adapter per tool (corbel via its MCP `serve`
    protocol over stdin/stdout, ripgrep, grep, and universal-ctags via its
    `--fields=+znKe` scope/end-line output).
  - `metrics.py` — multiset precision/recall/F1.
  - `report.py` — renders the run to Markdown and JSON.
  - `run_benchmark.py` — CLI entry point that ties it together.

## Running it

```
cargo build --release -p corbel
git clone https://github.com/sharkdp/hyperfine.git benchmarks/repos/hyperfine
git clone https://github.com/pallets/itsdangerous.git benchmarks/repos/itsdangerous
python3 benchmarks/harness/run_benchmark.py
```

Each golden set file pins the exact commit it was verified against
(`commit`). The harness checks the repository's actual `HEAD` against that
pinned commit and records a mismatch warning in the report if they diverge —
clone once and do not update the OSS repos in place, or the golden set no
longer describes the code being measured.

Use `--repo corbel`, `--repo hyperfine`, or `--repo itsdangerous` (repeatable)
to run a subset. Results are written as both
`benchmarks/results/benchmark-<timestamp>.{md,json}` and
`benchmarks/results/latest.{md,json}`.

## What's actually being compared

For every golden entry, all four tools are asked the same question — callers
of a known (name, file, line), callees of it, or where a bare name is
defined — using each tool's realistic best-practice usage (ripgrep/grep with
word-boundary patterns plus indentation-based enclosing-function detection;
ctags with its own scope/end-line fields; corbel via `get_symbol`). Every
run's report includes: an aggregate precision/recall/F1/time table per
repository, a full per-entry breakdown, and a `corbel failure causes` table
that buckets every scored corbel miss or false positive by root cause
(`unqualified_symbol_name`, `qualified_path_call_blind_spot`,
`name_collision_under_resolved`/`over_claimed`,
`dynamic_dispatch_no_static_target`, `high_fan_in_*`, or a residual
`other_*` bucket) rather than a single win/loss number.

Entries whose ground truth is genuinely ambiguous (dynamic dispatch / duck
typing with no single correct static target) are marked `ambiguous` in the
golden set and reported separately as a qualitative side-by-side of what each
tool actually returned — they are excluded from the scored aggregate tables
because there is no static answer to grade against, not because corbel or any
other tool "won" or "lost" there.
