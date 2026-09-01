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
  - `test_tool_adapters.py` — regression tests for the corbel adapter's
    truncation handling. Run with `python3 benchmarks/harness/test_tool_adapters.py`;
    skips the live-corbel tests automatically if `target/{release,debug}/corbel`
    hasn't been built.

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

## Why accuracy runs can't use corbel's default token budget

corbel's `get_symbol` MCP tool accepts an optional `token_budget` and
defaults to `DEFAULT_GET_SYMBOL_TOKEN_BUDGET` (8000) when the caller omits
it. That budget is split evenly across every matched symbol row, then each
row's share is split evenly again between its callers and its callees — so a
symbol with a large caller or callee list can have entries silently dropped
from the response. If the harness let that default apply during a
precision/recall run, a truncated response would be scored as if corbel had
genuinely failed to find those callers, understating corbel's own recall
against itself.

`tool_adapters.py` defines `BENCHMARK_TOKEN_BUDGET` (with its rationale
recorded right next to it as a string constant, not a comment, so it's
visible in the code and can be surfaced in the report) and passes it
explicitly on every `get_symbol` call used for accuracy scoring. Even so, the
harness never assumes truncation *can't* happen: every corbel adapter call
reports the `truncated`/`truncated_count` fields it got back, `run_benchmark.py`
collects them into a `truncated_cases` array (both at the top level of the
JSON report and per repository), and the CLI prints an explicit warning to
stderr — plus the Markdown report gets a banner at the top — if that array is
ever non-empty. This is deliberately not folded into the failure-cause table:
a truncated response is not a correctness failure, so it is reported
separately rather than silently inflating (or silently excusing) a recall
number.

There is currently no benchmark task that exercises `impact` or `find` —
every golden-set task type (`callers`, `callees`, `definition`) is answered
through a single `get_symbol` call, and no adapter in this harness calls
`impact` or `find` at all. If either is added later, the same
explicit-budget-plus-`truncated_cases` treatment applies: `impact` has its
own token budget with the identical splitting-into-silent-truncation risk,
and `find` additionally has a hard `limit` ceiling (`MAX_FIND_LIMIT = 200`,
enforced server-side as a rejection, not a silent clamp) that an accuracy-only
adapter must respect and never exceed.

## Measuring token usage as its own metric

This harness does not currently measure token usage as a metric — it only
uses `token_budget` as a dial to guarantee *no* truncation happens during
accuracy scoring. If token efficiency becomes something worth reporting
(e.g. "how many tokens does corbel spend to answer this versus grep's raw
output"), that must be a **separate run** that calls the adapters with
corbel's real default budget (or no explicit override at all) and measures
response size — not a number derived from the accuracy run's
`BENCHMARK_TOKEN_BUDGET`-forced, truncation-free responses, which would
misrepresent what a real MCP client sees by default. Mixing the two in one
run also breaks the current per-entry, per-tool table shape, since the
accuracy table's `precision`/`recall`/`f1` columns and a token-usage table's
column would answer different questions and would legitimately have to use
different budgets. This isn't implemented: it's a possible follow-up, not
built here.
