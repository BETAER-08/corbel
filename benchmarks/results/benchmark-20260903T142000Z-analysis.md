# Post-fix benchmark comparison — 2026-09-03 (after `unqualified_symbol_name` fix)

Companion to `benchmark-20260903T142000Z.json` / `.md`. Compares against the
baseline run `benchmark-20260903T092406Z` (preserved unmodified — see
`benchmark-20260903T092406Z-analysis.md`) after the owner-qualification fix
described in the prior session (`RawSymbol.owner`, `symbols.owner` column,
schema v3→v4, display-time `Owner::name`/`Owner.name` composition).

Reproduce with:

```
python3 benchmarks/harness/run_benchmark.py
```

## Pre-run checks

- `verify_commit`: all three repos still match their pinned commit exactly
  (unchanged from baseline — no drift).
- **Indexes rebuilt fully from scratch** (`.corbel/` deleted, reindexed) —
  required since `enclosing_symbol`'s meaning changed and the schema
  migration backfills `owner` as `NULL` on old rows, which would silently
  under-report if reused instead of rebuilt:

  | Repo | Index time | `index.db` size |
  | --- | --- | --- |
  | chevrotain | 3.18s | 824 KB (was 808 KB) |
  | hyperfine | 0.58s | 184 KB (unchanged) |
  | itsdangerous | 0.09s | 84 KB (unchanged) |

  Size growth is exactly what's expected from one new nullable `owner` TEXT
  column on the largest repo's symbol table; nothing else changed shape.
- Same conditions as the baseline run: T1 (callers) + T4 (definition) only,
  T2 (callees) excluded by default, `BENCHMARK_TOKEN_BUDGET = 1,000,000`.
- **Truncated cases: 0** (both runs) — no precision/recall number below is a
  truncation artifact in either run.

## 1. Overall precision / recall / F1 — before / after

Summed over all non-ambiguous T1+T4 entries across all three repos.

| Tool | Metric | Before (092406Z) | After (142000Z) | Δ |
| --- | --- | --- | --- | --- |
| **corbel** | TP / FP / FN | 200 / 305 / 308 | 358 / 147 / 150 | +158 / -158 / -158 |
| | Precision | 0.396 | **0.709** | +0.313 |
| | Recall | 0.394 | **0.705** | +0.311 |
| | F1 | 0.395 | **0.707** | +0.312 |
| grep | P / R / F1 | 0.472 / 0.640 / 0.543 | 0.472 / 0.640 / 0.543 | unchanged |
| ripgrep | P / R / F1 | 0.472 / 0.640 / 0.543 | 0.472 / 0.640 / 0.543 | unchanged |
| ripgrep+ctags | P / R / F1 | 0.617 / 0.844 / 0.713 | 0.617 / 0.844 / 0.713 | unchanged |

**grep/ripgrep/ripgrep+ctags numbers are byte-for-byte identical between the
two runs** — confirms the fix touched only corbel's code path and nothing in
the harness or golden set was altered to move these numbers. corbel's F1
went from clearly last (0.395, behind every baseline tool) to essentially
tied with the strongest baseline, `ripgrep+ctags` (0.707 vs 0.713) — no
longer an outlier, though still not a clean win against the hybrid.

Per-repo breakdown (corbel only):

| Repo | Before P/R | After P/R |
| --- | --- | --- |
| chevrotain | 0.483 / 0.456 | 0.868 / 0.820 |
| hyperfine | 0.414 / 0.345 | 0.586 / 0.488 |
| itsdangerous | 0.237 / 0.352 | 0.618 / 0.920 |

itsdangerous shows the largest relative jump (Python's class-heavy,
override-heavy style meant nearly every caller was a method before the fix).
hyperfine (Rust) shows the smallest jump — consistent with §2/§3 below: a
large share of hyperfine's remaining errors are not qualification-related at
all (mixin/trait-default-method and multiset-undercounting patterns
untouched by this fix).

## 2. Failure-cause classification — before / after

| Cause | Before (613 total) | After (297 total) |
| --- | --- | --- |
| `unqualified_symbol_name` | **335 (54.6%)** | **19 (6.4%)** |
| `other_missed_reference` | 118 | 118 |
| `other_spurious_match` | 78 | 78 |
| `name_collision_over_claimed` | 58 | 58 |
| `name_collision_under_resolved` | 13 | 13 |
| `dynamic_dispatch_no_static_target` | 9 | 9 |
| `qualified_path_call_blind_spot` | 2 | 2 |

`unqualified_symbol_name` dropped from 335 to 19 (94.3% reduction) — every
other cause bucket is **exactly unchanged**, byte-for-byte, which is the
expected signature of a fix that only touches owner-qualification and
nothing else (mixin/dynamic-dispatch/name-collision failures are orthogonal
and untouched, as predicted in the prior session's report).

### Why the remaining 19 are not (mostly) genuine qualification bugs

Investigated all 19 directly against source and ground truth. They split
into two distinct root causes, neither of which is "corbel still returns a
bare name where it should qualify":

**(a) 18 of 19 — a classifier blind spot in the harness itself, not a corbel
bug.** `run_benchmark.py`'s `classify_miss`/`classify_extra` tag a failure
`unqualified_symbol_name` whenever an item with the *same bare name and
file* exists on the other side of the diff — a heuristic written to detect
"right symbol, wrong qualification format." It does not check **multiset
multiplicity**. Two real, unrelated situations both produce "same bare name,
same file, count mismatch," and this heuristic cannot tell them apart:

- Golden-set entries where the same enclosing function calls the target
  **more than once** (e.g. `hyperfine-34`: `test_get_specified_command_names`
  calls `get_name()` 3 times at lines 569/570/572, recorded as 3 separate
  ground-truth entries with the identical `(enclosing_symbol, file)` key).
  corbel's `callers` list has **one row per distinct caller symbol**, not
  one row per call site — verified directly: `get_symbol("get_name", file="src/command.rs")`
  returns `test_get_specified_command_names` exactly once, correctly
  qualified (it's a free `#[test] fn`, correctly bare — no owner bug at
  all), but the multiset scorer counts 1 match + 2 "missing" against a
  ground truth expecting 3. `hyperfine-19`, `-20`, `-23`, `-24`,
  `chevrotain-har-050` show the identical pattern in the opposite direction
  (corbel's single row becomes several "extra" entries when *corbel's* raw
  output — before scoring — legitimately lists the same caller once, but a
  different symbol in the same file with the same bare name inflates the
  apparent count). This is a **real, separate, already-known corbel
  limitation** (per-caller-symbol granularity vs. per-call-site golden-set
  granularity) — but it is not what "unqualified_symbol_name" describes, and
  it existed identically in the baseline run (confirmed: these same seven
  entries already carried `unqualified_symbol_name` tags before the fix,
  e.g. `hyperfine-19` was 7-of-7 unqualified-tagged in the baseline and is
  still not fully resolved now, for this different reason).
- Confirmed programmatically across the **entire** run: of every `extra`
  item whose bare name matches a ground-truth entry's bare name, only 3
  (all `hyperfine-30`, see below) actually carry a *different* qualified
  name than what ground truth expects. All other bare-name coincidences are
  exact-string duplicates — a pure count mismatch, not a qualification
  mismatch.

**(b) 1 of 19 (3 occurrences) — a real, narrow remaining gap: Rust trait
default-method bodies.** `hyperfine-30`'s `MarkupExporter::table_results`
(`src/export/markup.rs:15`) is defined as a **default method body directly
inside `trait MarkupExporter { fn table_results(&self, ...) -> String {
...} }`** — not inside any `impl` block. `owner_of_definition` in
`rust.rs` only walks up looking for `impl_item`; it never checks for an
enclosing `trait_item`, so this method's `owner` comes back `None` and its
callers show up as bare `table_results` instead of
`MarkupExporter::table_results`. This is the **only** case in the entire
run where corbel emits a name that doesn't match ground truth's
qualification at all (as opposed to a correctly-qualified name whose count
doesn't match) — confirmed by directly diffing every `extra` entry's full
qualified string against ground truth's expected string for the same bare
name (see script output: 3 hits, all `table_results`). Not fixed in this
pass — flagged as a known, narrow follow-up (trait default methods are
comparatively rare; every other Rust owner path — inherent impl, generic
impl, reference-type trait impl, scoped-path impl — was already covered and
tested in the previous session).

## 3. Estimate vs. actual

The prior session's baseline analysis (§3) projected, from a paired-failure
model (166 of 335 `unqualified_symbol_name` misses paired 1:1 with a
matching extra, converting to TP if fixed):

| | TP | FP | FN | Precision | Recall |
| --- | --- | --- | --- | --- | --- |
| **Estimated** | 366 | 139 | 142 | ~0.725 | ~0.720 |
| **Actual (measured)** | 358 | 147 | 150 | 0.709 | 0.705 |

The estimate was close (within 8 TP / 1.6 precision points / 1.5 recall
points) but **optimistic by exactly the amount explained by §2(a) above**:
the estimate model treated every one of the 335 `unqualified_symbol_name`-
tagged failures as homogeneous — "caused by missing qualification, curable
by this fix." In reality, the harness's own tagging heuristic conflates two
different failure modes under one label (§2), and only the genuine
qualification-format subset was fixable by this change. The
multiset-multiplicity subset (§2a) was never going to close no matter how
the qualification bug was fixed, because it isn't a qualification bug — it's
corbel's caller list being one-row-per-symbol rather than one-row-per-call-
site. The estimate's error (8 TP short) is consistent with roughly that many
paired instances in the original 166 actually belonging to the multiplicity
bucket rather than the pure-formatting bucket — the model's flaw was
**trusting the harness's own failure-cause label as ground truth for "what
this fix will resolve," instead of independently verifying that each tagged
instance's root cause was actually the formatting bug being targeted.**
Lesson for future estimates: an automatic classifier's bucket boundaries are
themselves an approximation, and an estimate built on top of one inherits
its blind spots.

## 4. New failures introduced by the fix

Checked systematically, not just spot-checked: for every `extra` (false
positive) caller entry in the new run, whether its bare name matches a
ground-truth entry's bare name but with a **different owner** than ground
truth expects (i.e., corbel confidently attaching the *wrong* owner, rather
than no owner). Result:

**Zero cases of a wrong owner being attached.** The only 3 mismatches found
(all `hyperfine-30`/`table_results`, §2b) are corbel omitting an owner it
should have attached (a coverage gap), never attaching an incorrect one.
This was checked across all three repos, not just the ones discussed above.

No new failure-cause category is needed for "wrong owner" — the trait
default-method gap is folded into the existing `unqualified_symbol_name`
bucket in the raw classifier output (since the *symptom* — a bare name where
a qualified one was expected — is identical), but is called out separately
in §2b above since its root cause is distinct from the formatting bug this
fix targeted.

## Both runs preserved

- `benchmarks/results/benchmark-20260903T092406Z.{json,md}` +
  `-analysis.md` — baseline, **unmodified**, kept as evidence of the
  pre-fix state and as a check against "the harness was changed to match
  the desired result" (the byte-identical grep/ripgrep/ripgrep+ctags numbers
  above are the direct rebuttal to that concern).
- `benchmarks/results/benchmark-20260903T142000Z.{json,md}` + this file —
  post-fix run.
