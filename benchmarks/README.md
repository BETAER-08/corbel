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
    line" resolver, used by the grep and ripgrep adapters. Supports Rust,
    Python, and TypeScript/TSX/JavaScript (`TS_JS_LIMITATIONS.md` documents
    what the TS/JS heuristic intentionally does not catch).
  - `tool_adapters.py` — one adapter per tool (corbel via its MCP `serve`
    protocol over stdin/stdout, ripgrep, grep, and a `ripgrep+ctags` hybrid —
    see "What's actually being compared" below). All language-specific
    behavior (keywords, excluded directories, file extensions, ripgrep's
    `-t` type name, ctags' `--languages` name, and the definition-search
    regex) lives in one table, `LANGUAGES` in `tool_adapters.py`; add a
    language by adding one entry there.
  - `TS_JS_LIMITATIONS.md` — what the TS/JS regex-based definition matching
    and enclosing-scope resolution do not catch (generic type parameters,
    multi-line signatures, interface method signatures, etc).
  - `metrics.py` — multiset precision/recall/F1.
  - `report.py` — renders the run to Markdown and JSON.
  - `run_benchmark.py` — CLI entry point that ties it together.
  - `test_tool_adapters.py` — regression tests for the corbel adapter's
    truncation handling and for TS/JS language support (`TypeScriptAdapterTests`).
    Run with `python3 benchmarks/harness/test_tool_adapters.py`;
    skips the live-corbel tests automatically if `target/{release,debug}/corbel`
    hasn't been built.

## Verification methodology and its limits

Every entry in `benchmarks/golden/*.json` was verified by a single AI model
(Claude Sonnet 5, `verified_by: "claude-sonnet-5"` on all 120 entries) —
there is no human review of individual entries before commit. This is a
known, unresolved limitation: a lone verifier, human or model, can be
systematically wrong in a way repeated self-checks by the same verifier
won't catch, since the same blind spot reproduces the same wrong answer
every time.

What compensates, and what doesn't:

- Every caller/callee claim is cross-checked against an independent tool
  (ripgrep for candidate enumeration, an LSP server for reference/definition
  drafts) *and* against the actual source, read directly, before being
  accepted — neither tool alone is trusted. `benchmarks/goldenset/LSP_ERROR_TYPES.md`
  and `benchmarks/goldenset/TEXT_SEARCH_LIMITATIONS.md` catalogue concrete
  cases where each signal alone was wrong (up to a 31x overcount for
  ripgrep; 5 distinct failure modes for LSP drafts, across 3 languages).
- corbel is never consulted while building the golden set —
  `candidate_scanner.py` is grep/ctags-only by import-time construction, so
  the tool under test cannot influence what counts as an interesting
  candidate or what its ground truth is.
- Every `adversarial`-difficulty entry gets a second, independently-derived
  verification pass from a context-isolated subagent invocation that never
  sees the first pass's reasoning before producing its own answer.
- What this does **not** fix: a second pass by the same underlying model is
  not a second *kind* of verifier. A systematic bias in how Claude Sonnet 5
  reads a particular pattern would reproduce in an isolated re-run rather
  than get caught by it, the same way a human re-checking their own work
  twice doesn't catch their own blind spots. Full detail, including how
  "independent second pass" is concretely implemented, is in
  `benchmarks/goldenset/SCHEMA.md`'s "Verifier independence" section — this
  paragraph is a summary, not a substitute for reading it.

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
longer describes the code being measured. On mismatch, `verify_commit` (in
`run_benchmark.py`) also runs `git diff --stat <pinned>..HEAD` and reports
whether any *source* file changed (as opposed to only paths under
`.corbel/`, corbel's own index artifact directory): `commit_mismatch_detail`
in the JSON report, and an extra line under "Commit match" in the Markdown
report, tell you whether the mismatch is source drift (results are not
trustworthy, re-clone or re-pin) or artifact-only drift (safe to re-pin)
without a manual `git diff` investigation.

Use `--repo corbel`, `--repo hyperfine`, or `--repo itsdangerous` (repeatable)
to run a subset. Pass `--include-callees` to also run the callees (T2) task,
which is excluded by default — see "What's actually being compared" below.
Results are written as both
`benchmarks/results/benchmark-<timestamp>.{md,json}` and
`benchmarks/results/latest.{md,json}`.

Only the `.md` report and its hand-written `-analysis.md` are committed —
the raw `.json` (and `latest.{json,md}`) stay gitignored. The `.md` already
carries the full per-entry breakdown; the `.json` is a regenerable
intermediate (`python3 benchmarks/harness/run_benchmark.py` reproduces it),
and tracking regenerable multi-megabyte blobs in git history has a
permanent cost with no offsetting benefit.

## Run provenance: the three-stage chain

Three results in `benchmarks/results/` are kept as a deliberate provenance
chain, not just the latest number:

1. **[`benchmark-20260902T131822Z.md`](../results/benchmark-20260902T131822Z.md)** —
   marked **INVALID** at the top of the file. The harness itself had no
   TypeScript support yet (`tool_adapters.py` was a hardcoded rust/python
   table), so grep and ripgrep scored a flat 0.000/0.000/0.000 on the
   `chevrotain` (TypeScript) repo — not because they lost to corbel, but
   because they couldn't search TypeScript at all. Kept rather than deleted
   so the "before" state of the harness bug is checkable, not just
   asserted.
2. **[`benchmark-20260903T092406Z.md`](../results/benchmark-20260903T092406Z.md)** —
   harness's TypeScript support fixed; corbel itself not yet touched.
   corbel F1 0.395 here. This isolates the harness fix from any corbel
   change: everything that moved between stage 1 and stage 2 is the
   harness, not corbel.
3. **[`benchmark-20260903T142000Z.md`](../results/benchmark-20260903T142000Z.md)** —
   corbel's owner-qualification fix applied; harness untouched between
   stage 2 and stage 3. corbel F1 0.707. grep, ripgrep, and ripgrep+ctags
   score byte-identically to stage 2 here — direct evidence that this
   stage's improvement came from corbel's own code, not from adjusting the
   harness or golden set to move the number.

Reading the three in order shows two separate fixes landing in two separate
commits' worth of results, each isolable from the other — the point isn't
"corbel got better," it's that *which* fix caused *which* change is
checkable rather than asserted.

## What's actually being compared

For every golden entry, all four tools are asked the same question — callers
of a known (name, file, line), callees of it, or where a bare name is
defined — using each tool's realistic best-practice usage (ripgrep/grep with
word-boundary patterns plus indentation-based enclosing-function detection;
a `ripgrep+ctags` hybrid; corbel via `get_symbol`). Every
run's report includes: an aggregate precision/recall/F1/time table per
repository, a full per-entry breakdown, and a `corbel failure causes` table
that buckets every scored corbel miss or false positive by root cause
(`unqualified_symbol_name`, `qualified_path_call_blind_spot`,
`name_collision_under_resolved`/`over_claimed`,
`dynamic_dispatch_no_static_target`, `high_fan_in_*`, or a residual
`other_*` bucket) rather than a single win/loss number.

**Why `ripgrep+ctags`, not plain ctags.** Plain universal-ctags has no
call-site index, so it cannot answer the callers task at all — labeling that
"ctags" and reporting a bare 0 would be a strawman, and doesn't reflect how
developers actually use ctags in practice (paired with grep/ripgrep for call
sites, ctags for scope). Concretely: `ctags_find_callers` in
`tool_adapters.py` uses ripgrep (or grep as a fallback) to find call sites,
then uses `CtagsIndex` only to resolve which function/method each hit falls
inside (its scope) via ctags' `--fields=+znKe` scope/end-line output.
`ctags_find_callees` likewise uses ctags only for the function's end-line
boundary, then the same regex-based callee scanner every other adapter uses.
`ctags_find_definition` is the one task answered by ctags alone (a direct
tag-table lookup by name) — so it is the one row where this hybrid's numbers
are actually plain-ctags numbers.

Entries whose ground truth is genuinely ambiguous (dynamic dispatch / duck
typing with no single correct static target) are marked `ambiguous` in the
golden set and reported separately as a qualitative side-by-side of what each
tool actually returned — they are excluded from the scored aggregate tables
because there is no static answer to grade against, not because corbel or any
other tool "won" or "lost" there.

## Why the callees (T2) task is excluded by default

`run_benchmark.py` skips the callees task unless `--include-callees` is
passed. Reason: across the three current golden sets, callees ground truth
is non-empty for only 2/55 chevrotain entries, 2/33 hyperfine entries, and
4/21 itsdangerous entries — roughly 90% of scored callees tasks have an
empty expected answer (a leaf function that calls nothing). Scoring those
grades every tool on "did you correctly return nothing," not on real
retrieval capability, and a precision/recall of 0.000 across the board on
this task reads as "every tool failed" when the actual cause is an
underpopulated golden set, not tool failure. The code is not deleted — pass
`--include-callees` to run it, and the JSON report's top-level
`callees_task_included` / `callees_task_exclusion_reason` fields (and a
banner at the top of the Markdown report) record whether and why it was
skipped, so this is never a silent omission. Re-enable it once the golden
set's callees ground truth is filled in.

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
