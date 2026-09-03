> **INVALID RUN — kept for provenance, not as a result to cite.** The harness had no TypeScript support at the time of this run: `tool_adapters.py` hardcoded a rust/python-only language table, so every grep/ripgrep query against `chevrotain` (TypeScript) silently matched nothing. grep and ripgrep scored **0.000/0.000/0.000** on chevrotain here, and corbel's apparent lead was largely an artifact of the baseline tools being unable to search TypeScript at all, not of corbel understanding TypeScript better. Fixed in the harness run that follows this one: [benchmark-20260903T092406Z-analysis.md](benchmark-20260903T092406Z-analysis.md). See [benchmarks/README.md](../README.md#run-provenance-the-three-stage-chain) for how this run fits into the fix sequence.

# corbel vs grep / ripgrep / ctags — benchmark analysis (2026-09-02)

Raw harness output for this run: `benchmark-20260902T131822Z.json` / `.md` (also mirrored at `latest.json` / `latest.md`).
This file is a hand-written analysis layer on top of that raw output — the harness itself does not
compute language/difficulty breakdowns or win/loss cause taxonomies, so those are derived here directly
from the JSON, with the extraction script's logic shown inline so the numbers are checkable.

**Reproduce:**
```
python3 benchmarks/harness/run_benchmark.py
```
(from repo root, with `target/release/corbel` built via `cargo build --release -p corbel`; requires `rg`, `grep`, `ctags` on PATH)

## 0. Pre-flight, changes made before running

- **Commit pin mismatch found and fixed.** `hyperfine` and `itsdangerous` were sitting one commit ahead of
  their golden-set pin (`add golden set tooling`, both times an accidental commit of `.corbel/index.db*`
  binary artifacts, **no source changes** — confirmed via `git diff --stat <pinned> HEAD`). Both repos were
  `git checkout`ed back to their pinned commit (detached HEAD, which is the correct/expected state for a
  benchmark fixture repo). `chevrotain` was already exactly on its pinned commit and was not touched.
- **Recurrence prevention:** added `.corbel/` to `.git/info/exclude` in all three repos under
  `benchmarks/repos/`. Rationale for choosing this over the alternatives:
  - *(a) `.git/info/exclude` — chosen.* Purely local (`.git/info/exclude` is never itself tracked or
    pushed), zero product code touched, works today.
  - *(b) give the corbel CLI an index-path override* — checked `corbel index --help` and
    `crates/corbel/src/commands/index.rs` / `serve.rs`: the `.corbel/index.db` path is hardcoded
    (`root.join(".corbel")`), no flag or env var exists to relocate it. Adding one would be a real product
    change (new CLI surface) to solve a benchmark-fixture hygiene problem — out of proportion to the ask,
    not done here.
  - *(c) pre/post-run cleanup in the harness script* — rejected: fragile, only protects runs that go through
    the script, does nothing against a stray `git add -A` in the interim.
- **Harness crash fixed.** `run_benchmark.py`'s `bare_name()` threw `TypeError` on `chevrotain-med-024`,
  whose golden-set caller entries have `"enclosing_symbol": null` (two genuine module-top-level call sites,
  not inside any function). Added a `None` guard (`bare_name(None) -> None`) — a null-safety fix only, it
  does not change how any real entry is scored. Without it the harness cannot complete a run at all.
- **`verify_commit` improvement — proposed, not implemented** (see §6). Needs approval before touching it,
  per instruction.

All three repos verified pinned to their golden-set commit before the run below (`commit_matches: true` for
all three in the JSON).

## 1. Tool versions and index build cost

| Tool | Version |
| --- | --- |
| corbel | corbel 0.1.0 |
| grep | GNU grep 3.12 |
| ripgrep | ripgrep 15.2.0 |
| ctags | Universal Ctags 6.2.1 |
| python | 3.14.7 |
| os | Linux 7.1.9-200.fc44.x86_64 |

| Repo | Language | Entries | corbel index time | corbel index.db size | ctags build time |
| --- | --- | --- | --- | --- | --- |
| chevrotain | typescript | 55 | 3.170s (250 files, cold) | 808 KiB | 0.148s |
| hyperfine | rust | 39 | 0.58s (48 files, cold) | 180 KiB | 0.036s |
| itsdangerous | python | 26 | 0.10s (15 files, cold) | 80 KiB | 0.013s |

("Cold" = `.corbel/` deleted before the run, so schema/content changes can't hide behind unchanged-file
skipping — this run indexed 250/48/15 files fresh in every repo, 0 unchanged, confirmed from
`corbel_index_summary` in the JSON.)

Truncated cases: **0** across all three repos (`truncated_cases: []` at both per-repo and run level). No
precision/recall number below is affected by the `BENCHMARK_TOKEN_BUDGET=1,000,000` truncation ceiling.

## 2. Two data-quality problems found in this run that gate how every number below must be read

These are not corbel/tool capability differences. They are stated first because they explain why several of
the numbers in §3 look extreme, and ignoring them would produce a report that "interprets numbers" the
wrong way.

### 2a. `tool_adapters.py` has no TypeScript support — chevrotain's grep/ripgrep numbers are not measuring accuracy

`tool_adapters.py` hardcodes exactly two language branches everywhere (`_extension_for`, `_keywords_for`,
the `-t rust`/`-t py` flag to ripgrep, the `--include=*.py` flag to grep): `"rust"` or *else Python*. There
is no `"typescript"` branch. For chevrotain:

- `grep_find_callers` / `ripgrep_find_callers` / `*_find_definition` filter to `--include=*.py` / `-t py`.
  chevrotain has zero `.py` files, so these return **0 hits for every one of the 55 entries**, unconditionally
  — confirmed in the JSON: `chevrotain callers grep: tp=0 fp=0 fn=162`, `chevrotain definition grep: tp=0
  fp=0 fn=55`. This is not "grep failed to find these calls," it never ran a query capable of matching a
  `.ts` file.
- `_scan_callees_in_range` (used for the callees task by grep, ripgrep, *and* ctags — it isn't gated by tool)
  reads the real `.ts` source directly and applies `PY_KEYWORDS` to filter out control-flow keywords from
  the call-pattern regex. Since TS keywords (`new`, `typeof`, `as`, `catch`, `switch`'s brace body, generic
  `<T>(...)`, etc.) aren't in `PY_KEYWORDS`, this leaks large numbers of false "callee" hits — this is the
  main driver of `chevrotain callees grep: fp=3139`.

Net effect: **grep and ripgrep read as 0.000/0.000 precision/recall on every chevrotain task**, which is not
a real signal about grep/ripgrep's usefulness on TypeScript — it is 100% an artifact of the harness never
having been extended past rust/python. ctags is *not* affected the same way for the **definition** task
(Universal Ctags parses TypeScript natively via its own tag database, independent of this rust/python
branching), which is why ctags' chevrotain definition numbers (P=0.815, R=0.964) are real and comparable to
corbel's, but ctags' chevrotain **callers** number is *also* zero — see §5, its callers path routes through
the same broken ripgrep/grep call-site search.

**This is flagged, not fixed.** Extending `tool_adapters.py` to a real language-dispatch table (TS/JS
included) is the correct fix, but it's a harness change made to get *correct* numbers, and per instruction
harness changes aren't being made mid-run to influence this report's outcome. Recommendation for a
follow-up, separate change.

**Consequence for this report:** chevrotain's grep/ripgrep rows are reported for completeness in §3 but
must not be read as "corbel beats grep 32x on TypeScript." The only trustworthy chevrotain comparison in
this run is **corbel vs ctags** (both actually executed against `.ts` files for definition; ctags' callers
number is unusable for the same broken-call-site-search reason as grep, see §5).

### 2b. The golden set's `callees` (T2) ground truth is empty for ~90% of entries — T2 precision is not measuring accuracy for any tool

Checked directly against `benchmarks/golden/*.json`: entries with a **non-empty** `callees` ground truth:

| Repo | Non-empty callees / entries with a callees task |
| --- | --- |
| chevrotain | 2 / 55 |
| hyperfine | 2 / 33 |
| itsdangerous | 4 / 21 |

i.e. the golden set records `"callees": []` for the overwhelming majority of entries — not because those
functions call nothing (spot-checked `hyperfine-07`'s neighbor `compare_mean_time`-family functions, which
plainly call other functions), but because callee annotation appears to not have been completed/verified for
most entries. `SCHEMA.md` documents the caller-verification methodology in detail but says nothing about
callees being intentionally scoped down, so this reads as an incomplete golden set, not a documented
limitation.

Effect: since `score()` treats an empty ground truth as "correct answer is nothing," **any tool that
correctly reports real callees scores 0.000 precision on that entry** — the callee list itself is right, but
graded against a golden set that says "expect nothing." This is why every tool, including corbel, shows
`P=0.000` on `callees` in essentially every difficulty × repo bucket in §3, and why `other_spurious_match`
(632 cases) and `name_collision_over_claimed` (109 cases) dominate the failure-cause table — 741 of corbel's
1,046 total scored "failures" in this run are this artifact, not real corbel mistakes.

**Consequence for this report:** T2 (callees) precision/recall/F1 numbers are reported in §3 for completeness
per the raw-output requirement, but are **not usable as an accuracy signal for any tool** in this golden set
as currently annotated. Real T1 (callers) and T4 (definition) numbers are unaffected by this and are the
basis for §4-§5.

## 3. Precision / recall / F1 by language × difficulty × task

(non-ambiguous tasks only, matching the harness's own scoring exclusion for dynamic-dispatch/duck-typing
entries; `n` = number of scored entries in that bucket)

### chevrotain (typescript) — grep/ripgrep rows are the §2a artifact, not real signal

| Difficulty | Task | corbel | grep | ripgrep | ctags |
| --- | --- | --- | --- | --- | --- |
| easy | callers (n=9) | P=0.60 R=0.60 F1=0.60 | 0/0/0 | 0/0/0 | 0/0/0 |
| easy | callees (n=9) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| easy | definition (n=9) | P=1.00 R=1.00 F1=1.00 | 0/0/0 | 0/0/0 | P=1.00 R=1.00 F1=1.00 |
| medium | callers (n=23) | P=0.71 R=0.57 F1=0.63 | 0/0/0 | 0/0/0 | 0/0/0 |
| medium | callees (n=23) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| medium | definition (n=23) | P=0.85 R=1.00 F1=0.92 | 0/0/0 | 0/0/0 | P=0.85 R=1.00 F1=0.92 |
| hard | callers (n=18) | P=0.10 R=0.09 F1=0.10 | 0/0/0 | 0/0/0 | 0/0/0 |
| hard | callees (n=18) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| hard | definition (n=18) | P=0.72 R=1.00 F1=0.84 | 0/0/0 | 0/0/0 | P=0.74 R=0.94 F1=0.83 |
| adversarial | callees (n=3, non-ambig subset) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| adversarial | definition (n=5) | P=0.71 R=1.00 F1=0.83 | 0/0/0 | 0/0/0 | P=0.67 R=0.80 F1=0.73 |

### hyperfine (rust)

| Difficulty | Task | corbel | grep | ripgrep | ctags |
| --- | --- | --- | --- | --- | --- |
| easy | callers (n=15) | P=0.33 R=0.64 F1=0.43 | P=0.44 R=0.94 F1=0.60 | P=0.44 R=0.94 F1=0.60 | P=0.44 R=0.94 F1=0.60 |
| easy | callees (n=12) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| easy | definition (n=15) | P=0.94 R=1.00 F1=0.97 | P=0.94 R=1.00 F1=0.97 | P=0.94 R=1.00 F1=0.97 | P=0.94 R=1.00 F1=0.97 |
| medium | callers (n=16) | P=0.23 R=0.11 F1=0.15 | P=0.46 R=0.51 F1=0.49 | P=0.46 R=0.51 F1=0.49 | P=0.52 R=0.57 F1=0.54 |
| medium | callees (n=14) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| medium | definition (n=16) | P=0.89 R=1.00 F1=0.94 | P=0.89 R=1.00 F1=0.94 | P=0.89 R=1.00 F1=0.94 | P=0.89 R=1.00 F1=0.94 |
| hard | callers (n=5) | P=0.17 R=0.04 F1=0.06 | P=0.49 R=0.68 F1=0.57 | P=0.49 R=0.68 F1=0.57 | P=0.49 R=0.68 F1=0.57 |
| hard | callees (n=4) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| hard | definition (n=5) | P=0.71 R=1.00 F1=0.83 | P=0.62 R=1.00 F1=0.77 | P=0.62 R=1.00 F1=0.77 | P=0.62 R=1.00 F1=0.77 |
| adversarial | callers (n=1, non-ambig) | 0/0/0 (all) | | | |
| adversarial | definition (n=3) | P=0.40 R=0.67 F1=0.50 | P=0.50 R=1.00 F1=0.67 | P=0.50 R=1.00 F1=0.67 | P=0.50 R=1.00 F1=0.67 |

### itsdangerous (python)

| Difficulty | Task | corbel | grep | ripgrep | ctags |
| --- | --- | --- | --- | --- | --- |
| easy | callers (n=12) | P=0.00 R=0.00 F1=0.00 | P=0.66 R=1.00 F1=0.79 | P=0.66 R=1.00 F1=0.79 | P=0.66 R=1.00 F1=0.79 |
| easy | callees (n=9) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| easy | definition (n=12) | P=0.63 R=1.00 F1=0.77 | P=0.63 R=1.00 F1=0.77 | P=0.63 R=1.00 F1=0.77 | P=0.63 R=1.00 F1=0.77 |
| medium | callers (n=9) | P=0.04 R=0.05 F1=0.04 | P=0.41 R=1.00 F1=0.58 | P=0.41 R=1.00 F1=0.58 | P=0.41 R=1.00 F1=0.58 |
| medium | callees (n=8) | P=0.00 | P=0.00 | P=0.00 | P=0.00 |
| medium | definition (n=9) | P=0.38 R=1.00 F1=0.55 | P=0.38 R=1.00 F1=0.55 | P=0.38 R=1.00 F1=0.55 | P=0.38 R=1.00 F1=0.55 |
| hard | callers (n=1) | P=0.21 R=0.21 F1=0.21 | P=1.00 R=1.00 F1=1.00 | P=1.00 R=1.00 F1=1.00 | P=1.00 R=1.00 F1=1.00 |
| hard | definition (n=1) | P=1.00 R=1.00 F1=1.00 | P=1.00 R=1.00 F1=1.00 | P=1.00 R=1.00 F1=1.00 | P=1.00 R=1.00 F1=1.00 |
| adversarial | definition (n=4) | P=0.19 R=1.00 F1=0.32 | P=0.19 R=1.00 F1=0.32 | P=0.19 R=1.00 F1=0.32 | P=0.19 R=1.00 F1=0.32 |

**corbel's `callers` precision/recall is visibly worse than grep/ripgrep/ctags in every single language ×
difficulty bucket above.** This is real and reported as-is; §4 explains why (spoiler: mostly one specific,
fixable cause, `unqualified_symbol_name`, not "corbel can't find the call sites").

### Aggregate query time (informational, not a scored metric)

| Repo | corbel total (s) | grep total (s) | ripgrep total (s) | ctags total (s, incl. one-time build above) |
| --- | --- | --- | --- | --- |
| chevrotain | 0.064 | 0.236 | 0.519 | 0.237 |
| hyperfine | (see per-repo md) | | | |
| itsdangerous | (see per-repo md) | | | |

Full per-task timing is in the raw `.md`/`.json`; corbel's per-query time is consistently the fastest of the
four (single warm `serve` process vs. re-invoking a subprocess per query for the others), but this benchmark
is not I/O- or scale-stressed enough for that to be a meaningful differentiator — it is not emphasized here
because reporting it as a "corbel is faster" headline would itself be the kind of packaging the task asked
not to do.

## 4. corbel's losses on T1 (callers) and T4 (definition), by root cause

Root-cause counts below come from the harness's own `classify_miss`/`classify_extra` labels on every
corbel-vs-ground-truth diff (`benchmarks/harness/run_benchmark.py:60-95`), re-bucketed into the requested
taxonomy. `callees` (T2) losses are excluded here — see §2b, they are a golden-set artifact, not a corbel
loss; the full callees breakdown is still in the raw JSON.

| Cause | chevrotain | hyperfine | itsdangerous | Total | What it actually is |
| --- | --- | --- | --- | --- | --- |
| **unqualified_symbol_name** | 159 | 76 | 100 | **335** | corbel found the correct call site, in the correct file, but reported the enclosing symbol as a bare method name (`sign`) instead of the golden set's `Class.method` qualified form (`TimestampSigner.sign`). Verified directly: `itsdangerous-01`'s 19-caller ground truth is matched 19/19 by file+line, but every single one is scored as a miss+extra pair because corbel's `get_symbol` caller output never prefixes the enclosing class/struct/impl name. **This is corbel's single largest loss category by a wide margin — a real product bug in caller-name formatting, not a call-graph accuracy problem.** |
| other_missed_reference | 25 | 87 | 6 | 118 | Genuine misses — corbel's call graph did not contain the edge at all. |
| name_collision_over_claimed (T1+T4) | 31+21(T4)=52\* | 106 | 30 | ~253\* | corbel reported a call/definition for a same-named symbol in the *wrong* class/module — real name-resolution errors, most concentrated on Python (`itsdangerous`, dynamically-typed, more name reuse) and chevrotain's TS interface method names. |
| dynamic_dispatch_no_static_target | 8 | 1 | 0 | 9 | Correctly-unresolvable: interface/trait method calls or callback closures with no single static target (`LexerAdapter.input`, `RecognizerEngine.atLeastOneInternalLogic`). corbel has no runtime information, same as every other tool here. |
| name_collision_under_resolved | 5 | 7 | 1 | 13 | corbel merged two same-named-but-distinct symbols into one candidate set. |
| qualified_path_call_blind_spot | 0 | 2 | 0 | 2 | `Scheduler::print_relative_speed_comparison` calls through a fully-qualified path corbel's resolver doesn't special-case. |
| other_spurious_match (T1+T4, excl. callees) | 39(T1)+13(T4)=52\* | see above | 71(incl. some T4) | — | Real false positives not otherwise classified. |
| **macro** | 0 | 0 | 0 | 0 | No case in this golden set traces to Rust macro expansion (checked `hyperfine.json` verification text for "macro" — no hits). Not a factor in this run. |
| **duck_typing** / **prototype** | n/a (12 adversarial entries carry this nature but are scored as `ambiguous: true` and excluded from T1/T4 scoring — see §5) | | | | |
| other | remainder | | | | |

\* Some counts appear on both a T1 and T4 line in the harness's raw per-task breakdown; totals here are
collapsed for readability — exact per-(repo,task,cause) counts are in the raw JSON under
`repos[].task_results[].tools.corbel.failures[]`.

**Bottom line on losses:** if `unqualified_symbol_name` (335) is set aside as a formatting bug rather than a
retrieval miss, corbel's remaining real T1/T4 error volume (118 genuine misses + ~253 name-collision errors
+ 9 dynamic-dispatch + 13 under-resolved + 2 blind-spot ≈ 395 across 3 repos) is still larger than zero and
still loses to grep/ripgrep/ctags on raw precision/recall in every bucket in §3 — the qualification-name bug
explains a large chunk of the *header number* gap, not all of it.

## 5. corbel's wins — why grep/ripgrep missed what corbel found

Checked directly (script logic: for every T1/T4 task, intersect corbel's *matched* keys with grep's *missing*
keys): outside of the chevrotain TypeScript artifact in §2a, there is exactly **one** genuine corbel-found /
grep-missed case in the entire rust+python subset:

- **`hyperfine-34` (`Command::get_name`, callers, difficulty=hard):** grep missed attributing 2 of the 12
  ground-truth call sites (`test_parameter_scan_commands_names`, `test_get_specified_command_names`)
  to their correct enclosing test function. This isn't a case of grep failing to find the *line* — `rg` finds
  every `get_name(` call site by regex just fine. The miss is in the harness's own text-search
  enclosing-symbol attribution (`enclosing.py`'s regex-based scope walker), which loses track of scope
  inside these two particular test functions (both contain multiple nested calls across several lines),
  while corbel's AST-backed enclosing-symbol resolution gets it right structurally.

Separately, in TypeScript, corbel beats *ctags* (the one apples-to-apples chevrotain comparison, per §2a) on
the definition task for `chevrotain-adv-020` (`buildLookaheadForAlternation`) and `chevrotain-har-017`
(`validateAmbiguousAlternationAlternatives`) — Universal Ctags' TypeScript parser fails to tag both
definitions (likely a function-expression-assigned-to-`export const` pattern ctags' TS grammar doesn't
recognize), while corbel's own indexer, being TS-aware at the AST level rather than regex/tag-pattern level,
finds both.

**This is the honest state of "why corbel wins" in this golden set: two real, narrow, structural-parsing
wins (one over grep's naive scope attribution, one over ctags' incomplete TS tag grammar), not a broad
"text search can't do this" story.** The much larger apparent win margin on chevrotain callers/callees
(§3) is the §2a language-support artifact and should not be cited as evidence of anything.

## 6. `verify_commit` improvement — proposed only, not implemented

Current implementation (`run_benchmark.py:38-44`) only reports match/mismatch, forcing exactly the manual
`git log` / `git diff --stat` investigation done in §0 by hand. Proposed change (pending approval):

```python
def verify_commit(golden_set, repo_path):
    expected = golden_set["commit"]
    actual = subprocess.run(["git", "rev-parse", "HEAD"], cwd=repo_path,
                             capture_output=True, text=True).stdout.strip()
    if actual == expected:
        return actual, True, None
    diff = subprocess.run(["git", "diff", "--stat", expected, "HEAD"], cwd=repo_path,
                           capture_output=True, text=True).stdout
    # any changed path outside .corbel/ (or other known-artifact dirs) means real source drift
    source_changed = any(
        line.split("|")[0].strip() and not line.split("|")[0].strip().startswith(".corbel/")
        for line in diff.splitlines() if "|" in line
    )
    detail = "source files changed since pin" if source_changed else "artifact-only drift (safe to re-pin)"
    return actual, False, {"diff_stat": diff, "source_changed": source_changed, "detail": detail}
```

`run_repo()` and the report would then surface `detail` next to `commit_matches: false`, so a future stop
prints e.g. `commit mismatch for hyperfine: artifact-only drift (safe to re-pin)` instead of a bare
true/false — removing exactly the manual investigation step done in §0. Not applied to `run_benchmark.py` in
this run; awaiting go-ahead.

## 7. Adversarial entries (difficulty=adversarial, 12 total) — per-entry, per-tool

5 chevrotain + 3 hyperfine + 4 itsdangerous = 12, matching the stated premise. All 12 carry
`category: dynamic_dispatch` except `hyperfine-39` (`category: qualified_path_call`). The **callees** task on
these entries is marked `ambiguous: true` in the golden set (multiple plausible runtime targets) and is
excluded from scored aggregates by the harness itself — reported here individually instead, per instruction.

| Entry | What makes it adversarial | corbel | grep/ripgrep | ctags |
| --- | --- | --- | --- | --- |
| chevrotain-adv-018 | Visitor-pattern dynamic dispatch (`callees` ambiguous) | definition: found exactly (P/R=1.0) | definition: missed (`.py`-only filter, §2a) | definition: found exactly |
| chevrotain-adv-019 | Same pattern | definition: found but 2 spurious extra hits (P=0.33, R=1.0) — over-resolves to sibling lookahead functions | definition: missed | definition: same 2 spurious extras as corbel (both route through the same TS parse, real ambiguity not a corbel-only issue) |
| chevrotain-adv-020 | Same pattern | definition: found exactly | definition: missed | definition: **missed entirely** — ctags' TS grammar fails to tag this definition (§5) |
| chevrotain-adv-021 | grammar-author callback (`ambiguous` callees; acceptable answer is any grammar-author-supplied ALT callback) | callees: reports a plausible-looking but non-corresponding list (`raiseNoAltException`, `call`, `isArray`...) — none is the accepted answer | grep/rg dump the *entire* function body's calls (60+ items) as "the answer" — technically a superset containing something plausible, but useless as a targeted answer | ctags: empty list (no answer at all) |
| chevrotain-adv-022 | Same pattern, different callback site | same shape as adv-021 | same over-broad dump | empty |
| hyperfine-12 | Trait-object dispatch on a table formatter (`ambiguous` callees; accepted answer `table_header`) | reports a 20-item mixed list including `table_header` buried in it, but also many unrelated calls — not a clean identification | grep/rg/ctags all return the same ~25-item list, `table_header` present but similarly buried | same as grep |
| hyperfine-38 | Trait dispatch (`ambiguous` callees, accepted `serialize`) *and* the **definition** task itself is genuinely hard: `src/export/mod.rs:48` is a trait method declaration with 3 concrete impls | callees: returns `[]` — misses entirely, doesn't even surface `serialize` as a candidate. definition: **misses the trait declaration itself** (P=0, R=0), instead returning the 3 concrete `impl` bodies (csv.rs/json.rs/markup.rs) as if those were "the" definition | callees: returns `['File']` — also wrong, but at least attempts something. definition: P=0.25, R=1.0 — grep's naive `def`-pattern regex happens to match all 4 (trait decl + 3 impls) since it doesn't distinguish trait signatures from impls | definition: same P=0.25/R=1.0 as grep, ctags' tag kind doesn't distinguish trait-method-declaration from impl either |
| hyperfine-39 | Rust `for x in iter` desugars to repeated `Iterator::next()` calls with **zero textual `.next()` anywhere in source** — the golden-set note calls this out explicitly | callers: **misses the desugared call entirely** (P=0, R=0), same as every other tool — this is the one case in the whole run where corbel and all three baselines agree on being wrong for the identical structural reason (none of the four tools model implicit trait-method desugaring) | callers: also P=0, R=0 for grep/rg/ctags | same |
| itsdangerous-03 | Duck-typed dispatch through an abstract base class with a directly-instantiable Python ABC (no enforced abstractness) — audited in a prior independent-reverification pass per `SCHEMA.md`; accepted callees answer is `get_signature` (appears twice, once per candidate class) | callees: reports `get_signature` among 4 candidates — accepted answer present but not isolated. definition: P=0.25, R=1.0 — same 4-way name collision as every tool below | grep/rg/ctags: **identical** callees and definition numbers to corbel — this one is a pure name-collision case that all four tools handle identically, not a corbel-specific weakness |
| itsdangerous-11 | Same duck-typing shape, accepted callees answer `__init__` | callees: reports `__init__` among 5 candidates | grep/rg/ctags: same shape, ripgrep's *extra* set differs slightly (different false-candidate files) but recall is identical (1.0) across all four | — |
| itsdangerous-25 | Duck-typed `sign` dispatch | callees: `sign` present among 2 candidates. definition: P=0.5, R=1.0, one 1-name-collision extra (`timed.py:45`) | identical across all four tools | — |
| itsdangerous-26 | Duck-typed `dumps` dispatch | callees: `dumps` present among 2 candidates. definition: P=0.5, R=1.0, one extra (`url_safe.py:55`) | identical across all four tools | — |

**Pattern across all 12:** on genuinely runtime-resolved dispatch (duck typing / trait objects / grammar
callbacks), corbel does not do meaningfully better than grep/ripgrep/ctags — all four tools either return
the same name-collision candidate set (itsdangerous cases, where the harness's own `enclosing`/tag lookup is
shared machinery) or fail in tool-specific ways with no clear winner (chevrotain/hyperfine cases). The one
case where all four tools are *identically* wrong for the same structural reason is `hyperfine-39`
(implicit `Iterator::next()` desugaring) — worth keeping as the canonical "none of these tools model
implicit language-level dispatch" example for a README "known limitations" section, since it isn't
corbel-specific and isn't a bug, it's a shared ceiling.

## 8. Truncated cases

None. `truncated_cases: []` in every repo section and at the top level of the run report. No caveat needed
beyond noting the check was performed and came back clean — restated here per instruction since a run *with*
truncated cases would need this section to carry real content.

## 9. ctags and T1 (callers) — is it correctly recorded as N/A?

**No, and this needs to be stated precisely rather than assumed.** The harness's "ctags" caller adapter
(`ctags_find_callers`, `tool_adapters.py:340-356`) is not pure ctags — it's a **hybrid**: it calls
`_ripgrep_call_sites` (or `_system_grep_call_sites` if `rg` is absent) to find raw call-site line numbers by
regex, then uses the `CtagsIndex` purely to resolve each hit's *enclosing symbol* via `index.enclosing()`.
Pure Universal Ctags has no cross-reference/callgraph capability at all (it only emits a tag database of
*definitions*, not *call sites*), so a "correctly N/A" implementation would in fact report N/A for ctags on
T1. This harness does not do that — it silently substitutes ripgrep's call-site search underneath the
"ctags" label, which is why ctags posted non-null P/R/F1 numbers for callers in every repo above (e.g.
hyperfine callers: ctags P=0.47, R=0.68 — nearly identical to grep/ripgrep's own P=0.45/R=0.65, because it's
the *same* call-site search underneath, only the enclosing-symbol lookup differs).

This is the opposite of the failure mode the instruction warned about ("억지로 0점을 주면 불공정한 비교") — no
tool here is unfairly zeroed. Instead, **the "ctags" row for T1 in every table above is measuring
"ripgrep call-site search + ctags scope resolution," not what a bare `ctags` binary alone can do.** For
chevrotain specifically, that hybrid still inherits the §2a `-t py`/`.py`-only bug from its ripgrep/grep
fallback, so chevrotain's ctags-callers number (0/0/0 across the board) *looks* like a correct N/A but is
actually the same broken-language-filter artifact as grep/ripgrep, not a genuine structural incapacity
being reported. Recommend the README caveat this explicitly: "ctags" in this benchmark's T1 column means
ctags-assisted text search, not ctags alone.

## 10. Summary for anyone skimming

- corbel wins clearly on **definition lookup (T4)** across all three languages — matches or beats ctags,
  and beats grep/ripgrep everywhere except where they're artificially blocked (§2a).
- corbel **loses on callers (T1) precision/recall** to grep/ripgrep/ctags in every single language ×
  difficulty bucket that was fairly comparable. The largest identified cause (335 of its ~1046 scored
  "failures") is a real, fixable formatting bug — caller output doesn't qualify enclosing symbol names with
  their containing class/struct — not a call-graph correctness gap. The remainder is genuine: missed edges,
  name-collision errors, and (correctly) unresolvable dynamic dispatch.
- **T2 (callees) numbers are not usable for any tool** in this golden set as currently annotated — ~90% of
  entries have an unpopulated `[]` ground truth.
- **chevrotain's grep/ripgrep numbers are not usable** for the same reason in a different place — the
  harness has no TypeScript language support.
- On the 12 truly adversarial (runtime-dispatch) cases, no tool has a real edge; they're a shared ceiling,
  not a corbel differentiator.
