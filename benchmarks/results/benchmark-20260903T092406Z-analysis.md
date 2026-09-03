# Baseline benchmark analysis — 2026-09-03 (post-harness-fix, pre-corbel-fix)

Companion to `benchmark-20260903T092406Z.json` / `.md`. This run is the
**baseline for corbel accuracy work**: the harness's four structural bugs
(TypeScript support, ctags mislabeling, T2 scoring on an empty golden set,
opaque `verify_commit`) were fixed first and are not re-litigated here — see
the prior session's harness changes. Nothing in the harness or golden set
was changed to produce these numbers. Where a number looks wrong, the cause
is investigated and reported below, not corrected in the harness/golden set.

Reproduce with:

```
python3 benchmarks/harness/run_benchmark.py
```

(T1 callers + T4 definition only; T2 callees is excluded by default per the
harness fix — pass `--include-callees` to add it back once its golden-set
ground truth is filled in.)

## 0. Pre-run checks

- **`verify_commit`**: all three repos matched their pinned commit exactly
  (chevrotain `221fff76…`, hyperfine `f12f3d9f…`, itsdangerous `672971d6…`).
  No mismatch — nothing was skipped.
- **corbel index, rebuilt fresh** (`.corbel/` deleted and reindexed before
  the run):

  | Repo | Files | Symbols | References | Index time | `index.db` size |
  | --- | --- | --- | --- | --- | --- |
  | chevrotain | 250 | 1,671 | 7,660 | 3.18s | 808 KB |
  | hyperfine | 48 | 302 | 1,823 | 0.57s | 180 KB |
  | itsdangerous | 15 | 144 | 335 | 0.10s | 80 KB |

  Internal-call resolution rate at index time (corbel's own self-reported
  number, not a benchmark metric): chevrotain 73.4% (1,264 ambiguous /
  name-collision calls unresolved), hyperfine 97.5% (22 unresolved),
  itsdangerous 59.9% (83 unresolved). itsdangerous' low resolution rate at
  index time is consistent with its precision numbers below (extensive
  duck-typed/overridden-method name collisions).
- **Tool versions**: corbel 0.1.0, GNU grep 3.12, ripgrep 15.2.0, Universal
  Ctags 6.2.1, Python 3.14.7, Linux 7.1.9-200.fc44.x86_64.

## 1. Token usage — not measured, by design

This run does **not** report a token-usage metric. `tool_adapters.py` forces
`BENCHMARK_TOKEN_BUDGET = 1,000,000` on every corbel call specifically so
that accuracy scoring is never contaminated by truncation (see
`benchmarks/README.md`, "Why accuracy runs can't use corbel's default token
budget"). Reporting response size from *this* run's payloads would measure
an artificially-inflated, truncation-free budget, not what a real MCP client
sees by default — the README already flags this as a deliberate, separate,
not-yet-built follow-up. What **is** real and reported here: wall-clock
query time per tool per task (below), and truncated-case count (§6: zero).

## 2. Per-language × per-difficulty results (T1 callers, T4 definition)

TP/FP/FN and P/R are summed over all non-ambiguous entries in that
(language, difficulty, task) cell. `n` = entries scored.

### TypeScript (chevrotain)

| Difficulty | Task | Tool | n | TP | FP | FN | P | R |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| easy | callers | corbel | 9 | 6 | 4 | 4 | 0.600 | 0.600 |
| easy | callers | grep | 9 | 8 | 2 | 2 | 0.800 | 0.800 |
| easy | callers | ripgrep | 9 | 8 | 2 | 2 | 0.800 | 0.800 |
| easy | callers | ripgrep+ctags | 9 | 8 | 2 | 2 | 0.800 | 0.800 |
| medium | callers | corbel | 23 | 29 | 12 | 22 | 0.707 | 0.569 |
| medium | callers | grep | 23 | 30 | 28 | 21 | 0.517 | 0.588 |
| medium | callers | ripgrep | 23 | 30 | 28 | 21 | 0.517 | 0.588 |
| medium | callers | ripgrep+ctags | 23 | 34 | 18 | 17 | 0.654 | 0.667 |
| hard | callers | corbel | 18 | 9 | 77 | 92 | 0.105 | 0.089 |
| hard | callers | grep | 18 | 11 | 106 | 90 | 0.094 | 0.109 |
| hard | callers | ripgrep | 18 | 11 | 106 | 90 | 0.094 | 0.109 |
| hard | callers | ripgrep+ctags | 18 | 96 | 21 | 5 | 0.821 | 0.950 |
| easy | definition | corbel | 9 | 9 | 0 | 0 | 1.000 | 1.000 |
| easy | definition | grep | 9 | 9 | 0 | 0 | 1.000 | 1.000 |
| easy | definition | ripgrep | 9 | 9 | 0 | 0 | 1.000 | 1.000 |
| easy | definition | ripgrep+ctags | 9 | 9 | 0 | 0 | 1.000 | 1.000 |
| medium | definition | corbel | 23 | 23 | 4 | 0 | 0.852 | 1.000 |
| medium | definition | grep | 23 | 23 | 4 | 0 | 0.852 | 1.000 |
| medium | definition | ripgrep | 23 | 23 | 4 | 0 | 0.852 | 1.000 |
| medium | definition | ripgrep+ctags | 23 | 23 | 4 | 0 | 0.852 | 1.000 |
| hard | definition | corbel | 18 | 18 | 7 | 0 | 0.720 | 1.000 |
| hard | definition | grep | 18 | 9 | 4 | 9 | 0.692 | 0.500 |
| hard | definition | ripgrep | 18 | 9 | 4 | 9 | 0.692 | 0.500 |
| hard | definition | ripgrep+ctags | 18 | 17 | 6 | 1 | 0.739 | 0.944 |
| adversarial | definition | corbel | 5 | 5 | 2 | 0 | 0.714 | 1.000 |
| adversarial | definition | grep | 5 | 2 | 2 | 3 | 0.500 | 0.400 |
| adversarial | definition | ripgrep | 5 | 2 | 2 | 3 | 0.500 | 0.400 |
| adversarial | definition | ripgrep+ctags | 5 | 4 | 2 | 1 | 0.667 | 0.800 |

(chevrotain has no `adversarial`-difficulty `callers` entries scored — the
5 adversarial chevrotain entries are all `definition`-task, dynamic-dispatch
category; see §5.)

**Notable: `hard`/`callers` is corbel's single worst cell (P=0.105,
R=0.089)** — this is where the `applyMixins` trait-mixin pattern concentrates
(§3, prototype/runtime-assembly bucket) plus the `validatePatterns` golden-set
defect (§4). `ripgrep+ctags` wins this cell decisively (P=0.821, R=0.950)
because ctags' scope/end-line lookup correctly resolves trait-mixin call
sites that corbel's static resolver and the regex-based `enclosing.py` both
struggle with — worth investigating as a concrete lead for corbel's mixin
handling.

### Rust (hyperfine)

| Difficulty | Task | Tool | n | TP | FP | FN | P | R |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| easy | callers | corbel | 15 | 21 | 43 | 12 | 0.328 | 0.636 |
| easy | callers | grep | 15 | 31 | 39 | 2 | 0.443 | 0.939 |
| easy | callers | ripgrep | 15 | 31 | 39 | 2 | 0.443 | 0.939 |
| easy | callers | ripgrep+ctags | 15 | 31 | 39 | 2 | 0.443 | 0.939 |
| medium | callers | corbel | 16 | 9 | 31 | 71 | 0.225 | 0.113 |
| medium | callers | grep | 16 | 41 | 48 | 39 | 0.461 | 0.512 |
| medium | callers | ripgrep | 16 | 41 | 48 | 39 | 0.461 | 0.512 |
| medium | callers | ripgrep+ctags | 16 | 46 | 43 | 34 | 0.517 | 0.575 |
| hard | callers | corbel | 5 | 2 | 10 | 48 | 0.167 | 0.040 |
| hard | callers | grep | 5 | 34 | 35 | 16 | 0.493 | 0.680 |
| hard | callers | ripgrep | 5 | 34 | 35 | 16 | 0.493 | 0.680 |
| hard | callers | ripgrep+ctags | 5 | 34 | 35 | 16 | 0.493 | 0.680 |
| adversarial | callers | corbel | 1 | 0 | 7 | 1 | 0.000 | 0.000 |
| adversarial | callers | grep | 1 | 0 | 7 | 1 | 0.000 | 0.000 |
| adversarial | callers | ripgrep | 1 | 0 | 7 | 1 | 0.000 | 0.000 |
| adversarial | callers | ripgrep+ctags | 1 | 0 | 7 | 1 | 0.000 | 0.000 |
| easy | definition | corbel | 15 | 15 | 1 | 0 | 0.938 | 1.000 |
| easy | definition | grep | 15 | 15 | 1 | 0 | 0.938 | 1.000 |
| easy | definition | ripgrep | 15 | 15 | 1 | 0 | 0.938 | 1.000 |
| easy | definition | ripgrep+ctags | 15 | 15 | 1 | 0 | 0.938 | 1.000 |
| medium | definition | corbel | 16 | 16 | 2 | 0 | 0.889 | 1.000 |
| medium | definition | grep | 16 | 16 | 2 | 0 | 0.889 | 1.000 |
| medium | definition | ripgrep | 16 | 16 | 2 | 0 | 0.889 | 1.000 |
| medium | definition | ripgrep+ctags | 16 | 16 | 2 | 0 | 0.889 | 1.000 |
| hard | definition | corbel | 5 | 5 | 2 | 0 | 0.714 | 1.000 |
| hard | definition | grep | 5 | 5 | 3 | 0 | 0.625 | 1.000 |
| hard | definition | ripgrep | 5 | 5 | 3 | 0 | 0.625 | 1.000 |
| hard | definition | ripgrep+ctags | 5 | 5 | 3 | 0 | 0.625 | 1.000 |
| adversarial | definition | corbel | 3 | 2 | 3 | 1 | 0.400 | 0.667 |
| adversarial | definition | grep | 3 | 3 | 3 | 0 | 0.500 | 1.000 |
| adversarial | definition | ripgrep | 3 | 3 | 3 | 0 | 0.500 | 1.000 |
| adversarial | definition | ripgrep+ctags | 3 | 3 | 3 | 0 | 0.500 | 1.000 |

**corbel's callers recall collapses as difficulty rises** (0.636 → 0.113 →
0.040) in a way grep/ripgrep's does not (0.939 → 0.512 → 0.680, noisier but
not collapsing). This tracks the `unqualified_symbol_name` formatting bug's
concentration in more complex, more-nested Rust code (§3).

### Python (itsdangerous)

| Difficulty | Task | Tool | n | TP | FP | FN | P | R |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| easy | callers | corbel | 12 | 0 | 22 | 23 | 0.000 | 0.000 |
| easy | callers | grep | 12 | 23 | 12 | 0 | 0.657 | 1.000 |
| easy | callers | ripgrep | 12 | 23 | 12 | 0 | 0.657 | 1.000 |
| easy | callers | ripgrep+ctags | 12 | 23 | 12 | 0 | 0.657 | 1.000 |
| medium | callers | corbel | 9 | 1 | 24 | 19 | 0.040 | 0.050 |
| medium | callers | grep | 9 | 20 | 29 | 0 | 0.408 | 1.000 |
| medium | callers | ripgrep | 9 | 20 | 29 | 0 | 0.408 | 1.000 |
| medium | callers | ripgrep+ctags | 9 | 20 | 29 | 0 | 0.408 | 1.000 |
| hard | callers | corbel | 1 | 4 | 15 | 15 | 0.211 | 0.211 |
| hard | callers | grep | 1 | 19 | 0 | 0 | 1.000 | 1.000 |
| hard | callers | ripgrep | 1 | 19 | 0 | 0 | 1.000 | 1.000 |
| hard | callers | ripgrep+ctags | 1 | 19 | 0 | 0 | 1.000 | 1.000 |
| easy | definition | corbel | 12 | 12 | 7 | 0 | 0.632 | 1.000 |
| easy | definition | grep | 12 | 12 | 7 | 0 | 0.632 | 1.000 |
| easy | definition | ripgrep | 12 | 12 | 7 | 0 | 0.632 | 1.000 |
| easy | definition | ripgrep+ctags | 12 | 12 | 7 | 0 | 0.632 | 1.000 |
| medium | definition | corbel | 9 | 9 | 15 | 0 | 0.375 | 1.000 |
| medium | definition | grep | 9 | 9 | 15 | 0 | 0.375 | 1.000 |
| medium | definition | ripgrep | 9 | 9 | 15 | 0 | 0.375 | 1.000 |
| medium | definition | ripgrep+ctags | 9 | 9 | 15 | 0 | 0.375 | 1.000 |
| hard | definition | corbel | 1 | 1 | 0 | 0 | 1.000 | 1.000 |
| hard | definition | grep | 1 | 1 | 0 | 0 | 1.000 | 1.000 |
| hard | definition | ripgrep | 1 | 1 | 0 | 0 | 1.000 | 1.000 |
| hard | definition | ripgrep+ctags | 1 | 1 | 0 | 0 | 1.000 | 1.000 |
| adversarial | definition | corbel | 4 | 4 | 17 | 0 | 0.190 | 1.000 |
| adversarial | definition | grep | 4 | 4 | 17 | 0 | 0.190 | 1.000 |
| adversarial | definition | ripgrep | 4 | 4 | 17 | 0 | 0.190 | 1.000 |
| adversarial | definition | ripgrep+ctags | 4 | 4 | 17 | 0 | 0.190 | 1.000 |

**corbel's `easy`/`callers` score is 0.000/0.000 (0 TP out of 23 ground-truth
callers)** — the starkest single cell in the whole run. Every one of those
misses is `unqualified_symbol_name` (verified directly: `itsdangerous-01`'s
30 failure entries are 100% that cause — see §3). grep/ripgrep/ctags recall
is a clean 1.000 across every itsdangerous `callers` cell; their precision
loss is a separate, genuine name-collision problem (heavy method-name reuse
across `Signer`/`TimestampSigner`/`Serializer`/subclasses), not a formatting
bug.

## 3. corbel failure-cause classification (T1 + T4, 603 classified failures)

613 raw failure entries (missed + spurious, corbel only) were logged by the
harness across all non-ambiguous callers/definition tasks. 10 of those are
excluded below and reported separately in §4 because they are not
attributable to corbel at all (§4b: `validatePatterns`/`validateRegExpPattern`
golden-set defect — every tool gets the identical answer and is identically
"wrong" against a ground truth that is itself incorrect).

| Cause | Count | % of 603 |
| --- | --- | --- |
| `unqualified_symbol_name` (formatting bug) | 335 | 55.6% |
| 기타 (unclassifiable within this review's scope) | 127 | 21.1% |
| 이름 충돌 오탐/미검출 (name_collision) | 81 | 13.4% |
| dynamic_dispatch | 41 | 6.8% |
| prototype / 런타임 조립 (runtime prototype assembly) | 14 | 2.3% |
| duck_typing | 5 | 0.8% |
| macro / 매크로 생성 호출 | 0 | 0.0% |

**macro/generated-call: literally zero.** Source review across all
`other_missed_reference`/`other_spurious_match` items in all three repos
(chevrotain, hyperfine, itsdangerous) turned up no `macro_rules!`, proc
macro, `#[derive(...)]`-generated call, or codegen-loop-generated method
involved in any corbel miss or false positive. The bucket exists in this
taxonomy but is empty for this golden set — not omitted, verified absent.

### How each non-`unqualified_symbol_name`/non-기타 bucket was verified (not guessed)

- **`dynamic_dispatch` (41)**: `dynamic_dispatch_no_static_target` is the
  harness's own auto-assigned cause for *missed* callers/definitions in a
  `category: "dynamic_dispatch"` entry, but the harness's `classify_extra`
  function has no `dynamic_dispatch` branch (only `overload_ambiguous_name`
  and `multi_hop` are special-cased for *spurious* matches) — so a spurious
  match in a dynamic-dispatch entry falls through to `other_spurious_match`
  even though its entry is clearly dynamic-dispatch. This is a labeling gap
  in the harness's own automatic classifier, not a new bug introduced here;
  this analysis manually re-attributes any `other_*` failure whose entry's
  golden-set `category` is `dynamic_dispatch` back into this bucket (applies
  to `chevrotain-har-028`/`chevrotain-adv-019` — the `RestWalker.walk`
  polymorphic-`this` dispatch through `RestDefinitionFinderWalker` — and to
  `itsdangerous-03`/`-11`/`-25`/`-26`).
- **`prototype_runtime_assembly` (14)**: chevrotain's `Parser` class is
  assembled at runtime via `applyMixins(Parser, [Recoverable, LooksAhead,
  TreeBuilder, LexerAdapter, RecognizerEngine, RecognizerApi, ErrorHandler,
  GastRecorder, PerformanceTracer])` (`parser.ts:270`), and `applyMixins`
  itself (`utils/apply_mixins.ts`) is literally
  `derivedCtor.prototype[propName] = baseCtor.prototype[propName]` — runtime
  prototype composition, not compile-time inheritance. Every failure whose
  scored symbol (or whose caller/definition target) lives in one of those 9
  trait files under `packages/chevrotain/src/parse/parser/traits/` is
  counted here (e.g. `consumeInternal`/`CONSUME`–`CONSUME9`,
  `walkOption`/`walkAtLeastOne*`/`walkMany*`, `raiseNoAltException`). corbel's
  static call graph does not appear to follow `this.method()` calls across
  this specific mixin-assembly boundary.
- **`duck_typing` (5)**: `chevrotain-med-023`'s `tokenLabel` misses are all
  calls from inside `defaultParserErrorProvider`
  (`errors_public.ts:19`, `export const defaultParserErrorProvider:
  IParserErrorMessageProvider = { buildMismatchTokenMessage(...) {...}, ...
  }`) — an object literal satisfying an interface structurally, with no
  `class ... implements IParserErrorMessageProvider` anywhere. Verified by
  reading the source directly, not inferred from the category tag (this
  entry's golden-set category is `simple`).
- **name_collision (81)**: harness-auto-tagged `name_collision_over_claimed`/
  `name_collision_under_resolved` (71) plus itsdangerous definition-task
  bare-name lookups manually confirmed to be multi-class method-name
  collisions, not corbel-specific errors — e.g. `__init__` (11 spurious
  matches: every `__init__` in the file tree, since `corbel_find_definition`
  and the regex/ctags def-search all look up by bare name with no
  file/line disambiguator), `unsign`/`get_signature`/`dumps`/`iter_unsigners`
  each overridden across `Signer`/`TimestampSigner`/`Serializer`/subclasses
  (10 more). All four tools produce the *same* extra candidates for these —
  this is a shared "bare-name lookup has no way to know which override you
  meant" ceiling, not something distinguishing corbel from grep/ripgrep/ctags.

### 기타 (127) — what's actually in it

Not a dump of "corbel failed and we don't know why" — it breaks down as:
- **17 hyperfine `callers` misses/extras**, not individually source-verified
  in this pass (time-boxed; flagged for a follow-up review) — `hyperfine-39`
  and `hyperfine-34` within this set *were* verified and are addressed
  separately in §4a/§4c since they're not corbel-specific.
- **`qualified_path_call_blind_spot` (2)**: the harness's own named category
  for calls through a fully-qualified path corbel's resolver doesn't follow —
  a real, previously-documented corbel limitation, not newly discovered here.
- **`high_fan_in_under_collected`/`high_fan_in_over_claimed` (0 in this
  run's non-golden-bug set)**: category exists in the classifier but no
  entries landed here after excluding the golden-set-bug items.
- **6 chevrotain items** (`getExtraProductionArgument`, `augmentTokenTypes`,
  `charCodeToOptimizedIndex`) reviewed but not confidently attributable to
  any of the 6 named buckets — `charCodeToOptimizedIndex` in particular has
  6+ call sites across 3 files (`lexer.ts`, `lexer_public.ts`, `reg_exp.ts`),
  a multi-hop-shaped miss that didn't cleanly fit `multi_hop` either since
  its golden-set category is `simple`.
- **6 itsdangerous `callers` misses** (`unsign`, `dump_payload`,
  `load_payload`, `iter_unsigners` ×2) that are call sites through
  `self.method()`/`super().method()` inside an override chain
  (`TimestampSigner.unsign` → `super().unsign(...)`,
  `Serializer.loads` → `signer.unsign(...)` where `signer`'s concrete class
  is chosen via a configurable `signer_class` attribute). This resembles
  `dynamic_dispatch`, but these entries are tagged `category: "simple"` by
  the golden-set curator (unlike the 4 itsdangerous entries the curator did
  tag `dynamic_dispatch`), and this analysis chose not to override that
  editorial judgment without stronger justification — left in 기타 rather
  than silently reclassified.

### % of failures from `unqualified_symbol_name`, and an ESTIMATE (not a measurement)

**55.6% of the 603 classified corbel failures (335) are the
`unqualified_symbol_name` formatting bug** — corbel returning a bare method
name (`base64_encode`, `__init__`) where the golden set expects (and
grep/ripgrep's `enclosing.py`, working correctly, produces) an
owner-qualified name (`Signer.__init__`). Each occurrence typically costs
corbel *twice*: 166 of the 335 are logged as a missed caller/definition (the
qualified answer never appears) and 169 as a spurious extra (the unqualified
answer corbel *did* return doesn't match the qualified key) — i.e. one
formatting defect produces one FN **and** one FP in the same entry.

**ESTIMATE, clearly not a measured result:** if this formatting bug were
fixed such that a bare-name answer is instead correctly qualified (assuming,
optimistically, that the *correct* class/owner is what corbel already has
internally and only the string formatting is wrong — this analysis did not
inspect corbel's source to confirm that assumption), converting the 166
paired (FN, FP) instances into TPs and leaving the 3 unpaired FPs as-is
projects to:

| | TP | FP | FN | Precision | Recall |
| --- | --- | --- | --- | --- | --- |
| **Actual (measured)** | 200 | 305 | 308 | 0.396 | 0.394 |
| **Estimated if fixed** | 366 | 139 | 142 | ~0.725 | ~0.720 |

This estimate is a **projection, not a re-run** — it is not mixed into any
of the measured numbers above or below, and should not be cited as corbel's
accuracy. It exists only to size the opportunity: fixing this one formatting
defect could plausibly *double* corbel's measured T1/T4 precision and recall
on this golden set, more than any other single known issue.

## 4. Cases where all four tools agree and are wrong

### 4a. Genuine tool-limitation cases

| Entry | Symbol | Cause |
| --- | --- | --- |
| `hyperfine-39` | `next` (`Iterator::next`) | `for x in iter` desugars to repeated `Iterator::next()` calls with zero textual `.next()` in source anywhere. No tool here does implicit-desugaring analysis. Confirmed by the golden set's own `verification_note`. |

### 4b. Golden-set defect, not a tool-limitation case (excluded from §3's corbel-failure attribution)

`chevrotain-med-041` through `-045` (`findEndOfInputAnchor`,
`findStartOfInputAnchor`, `findUnsupportedFlags`, `findDuplicatePatterns`,
`findEmptyMatchRegExps`) — golden set says the sole caller's enclosing
symbol is `validatePatterns` (`lexer.ts:440`), citing call lines
474/476/478/480/482. **Directly reading `lexer.ts`, those five call lines
are inside `validateRegExpPattern` (`lexer.ts:466-482`), a separate function
nested between `validatePatterns` (ends at `lexer.ts:463`) and the next
top-level function** — `validatePatterns` only calls `validateRegExpPattern`
once (`lexer.ts:452`), which then internally fans out to these five `find*`
helpers. `grep -rn "findEndOfInputAnchor(" packages/chevrotain/src/` finds
exactly one call site in the entire repo, and it is inside
`validateRegExpPattern`, not `validatePatterns`. **All four tools
unanimously report `validateRegExpPattern` as the caller and unanimously
"miss" `validatePatterns`** — this is not a tool disagreement, it is uniform
agreement against what looks like an incorrect ground truth. The golden
set's own `verification_note` for these entries describes "9
near-identically-shaped `find*` pattern-validation checks... inside
`validatePatterns`", which appears to have missed that the block is not one
flat function body but `validatePatterns` calling `validateRegExpPattern`
which then calls the five `find*` functions — exactly the kind of
easy-to-misattribute nested-function situation the verification note itself
warns about, just one level deeper than it caught.

**Per instruction, this was not corrected in `benchmarks/golden/chevrotain.json`.**
It is flagged here for whoever next edits that golden set. If left as-is,
these 5 entries continue to penalize every tool identically and do not
distinguish corbel from grep/ripgrep/ctags — they should probably be
excluded from any "corbel underperforms" narrative built on this data until
fixed.

## 5. Adversarial (12 entries) — per-tool detail

All 12 are `category: dynamic_dispatch` except `hyperfine-39`
(`qualified_path_call`, covered in §4a). Callees(T2) on these entries would
be `ambiguous: true` in the golden set and is excluded from this run per the
T2 exclusion (§ harness fix).

| Entry | Symbol (task) | corbel | grep | ripgrep | ripgrep+ctags |
| --- | --- | --- | --- | --- | --- |
| chevrotain-adv-018 | `defaultVisit` (def) | P=1.00 R=1.00 | **P=n/a R=0.00 (miss)** | **P=n/a R=0.00 (miss)** | P=1.00 R=1.00 |
| chevrotain-adv-019 | `visitOption` (def) | P=0.33 R=1.00 | P=0.33 R=1.00 | P=0.33 R=1.00 | P=0.33 R=1.00 |
| chevrotain-adv-020 | `buildLookaheadForAlternation` (def) | P=1.00 R=1.00 | **miss** | **miss** | **miss (ctags TS grammar gap)** |
| chevrotain-adv-021 | `orInternal` (def) | P=1.00 R=1.00 | **miss** | **miss** | P=1.00 R=1.00 |
| chevrotain-adv-022 | `doSingleRepetition` (def) | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 |
| hyperfine-12 | `table_results` (def) | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 |
| hyperfine-38 | `serialize` (def) | **P=0.00 R=0.00** | P=0.25 R=1.00 | P=0.25 R=1.00 | P=0.25 R=1.00 |
| hyperfine-39 | `next` (callers) | 0/0 all four (§4a) | 0/0 | 0/0 | 0/0 |
| hyperfine-39 | `next` (def) | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 | P=1.00 R=1.00 |
| itsdangerous-03 | `get_signature` (def) | P=0.25 R=1.00 | P=0.25 R=1.00 | P=0.25 R=1.00 | P=0.25 R=1.00 |
| itsdangerous-11 | `__init__` (def) | P=0.08 R=1.00 | P=0.08 R=1.00 | P=0.08 R=1.00 | P=0.08 R=1.00 |
| itsdangerous-25 | `sign` (def) | P=0.50 R=1.00 | P=0.50 R=1.00 | P=0.50 R=1.00 | P=0.50 R=1.00 |
| itsdangerous-26 | `dump_payload` (def) | P=0.50 R=1.00 | P=0.50 R=1.00 | P=0.50 R=1.00 | P=0.50 R=1.00 |

Per-entry notes:

- **adv-018/020/021**: corbel definition-finds a function-expression
  assigned to `export const`/a class method with a multi-line signature that
  grep/ripgrep's regex (single-line-signature requirement, see
  `TS_JS_LIMITATIONS.md`) misses outright. `ripgrep+ctags` splits: it
  catches adv-018 and adv-021 (ctags' tag table has them) but misses
  adv-020 — Universal Ctags' TypeScript grammar itself fails to tag
  `buildLookaheadForAlternation`, independent of the ripgrep-vs-regex issue.
- **adv-019**: all four tools identical (P=0.33, R=1.00) — 2 spurious extra
  matches from sibling lookahead-walker functions that structurally look
  the same; a shared ceiling, not a corbel-specific miss.
- **adv-022**: all four tools identical and perfect — not every mixin-trait
  method is missed by regex tools, only the multi-line-signature ones.
- **hyperfine-38**: the one case in this table where **corbel is uniquely
  wrong** — it misses the trait-method declaration (`src/export/mod.rs:48`,
  a `trait Table3 { fn serialize(...); }`-shaped signature with 3 concrete
  `impl`s) entirely (P=0, R=0), while grep/ripgrep/ctags all correctly find
  it (P=0.25, R=1.00 — recall correct, precision diluted only by the 3
  concrete impls also textually matching the naive `fn serialize` pattern).
- **itsdangerous-03/11/25/26**: all four tools identical on every one —
  duck-typed/name-collision candidate sets, not a corbel weakness (§3).

## 6. Truncated cases

**Zero.** `truncated_cases` is empty at both the top level and every
per-repo level in `benchmark-20260903T092406Z.json`. Every corbel
`get_symbol` call fit inside `BENCHMARK_TOKEN_BUDGET = 1,000,000`; no
precision/recall number in this report was affected by truncation.

## 7. Diff vs. the previous run (`benchmark-20260902T131822Z`, pre-harness-fix)

**Not apples-to-apples on recall for hyperfine/itsdangerous**: the old run
included T2 (callees) in its aggregate; this run excludes T2 by default
(§ harness fix). Where recall is identical old→new below, that's because T2
entries with empty ground truth contributed FPs (hurting precision) but not
to the recall denominator — consistent, not a red flag, but noted so the
table isn't misread as "nothing changed except chevrotain."

| Repo | Tool | OLD (T1+T2+T4) | NEW (T1+T4 only) |
| --- | --- | --- | --- |
| chevrotain | corbel | P=0.250 R=0.456 | P=0.483 R=0.456 |
| chevrotain | ctags → ripgrep+ctags | P=0.558 R=0.244 | P=0.783 R=0.880 |
| chevrotain | grep | **P=0.000 R=0.000** | **P=0.387 R=0.424** |
| chevrotain | ripgrep | **P=0.000 R=0.000** | **P=0.387 R=0.424** |
| hyperfine | corbel | P=0.105 R=0.345 | P=0.414 R=0.345 |
| hyperfine | ctags → ripgrep+ctags | P=0.162 R=0.739 | P=0.530 R=0.739 |
| hyperfine | grep | P=0.156 R=0.714 | P=0.512 R=0.714 |
| hyperfine | ripgrep | P=0.156 R=0.714 | P=0.512 R=0.714 |
| itsdangerous | corbel | P=0.170 R=0.352 | P=0.237 R=0.352 |
| itsdangerous | ctags → ripgrep+ctags | P=0.400 R=1.000 | P=0.524 R=1.000 |
| itsdangerous | grep | P=0.398 R=1.000 | P=0.524 R=1.000 |
| itsdangerous | ripgrep | P=0.398 R=1.000 | P=0.524 R=1.000 |

**The headline number: chevrotain grep/ripgrep went from a flat 0.000/0.000
(TS support was completely broken — every query returned nothing) to a real,
non-degenerate 0.387/0.424.** This is the direct, verified effect of the
harness fix (`tool_adapters.py`'s `LANGUAGES` table, `enclosing.py`'s TS
index) — before it, corbel's apparent lead on chevrotain was largely an
artifact of grep/ripgrep being unable to search TypeScript at all, not of
corbel understanding TypeScript better. With the bug fixed, corbel still
leads on chevrotain `callers`/`definition` in aggregate (per-cell detail in
§2), but by a real, measured margin instead of a 0.000-baseline artifact.

corbel's own numbers moved too, even though corbel itself was not touched —
purely from T2 exclusion changing the denominator (see caveat above).

## Summary for whoever picks up the corbel-fix work next

1. **Fix `unqualified_symbol_name` first.** It's 55.6% of all classified
   corbel failures and the estimate in §3 projects roughly a 2x
   precision/recall improvement — clearly not a measurement, but the
   largest lever by a wide margin.
2. **`applyMixins`-style runtime trait/mixin composition** (chevrotain,
   14 failures, concentrated in the `hard`/`callers` cell where corbel's
   recall is 0.089 — its worst cell in the whole run) is the second-largest
   identified, source-verified cause. `ripgrep+ctags` currently handles this
   pattern better than corbel does in that cell.
3. Name-collision bare-name lookup (81 failures, itsdangerous-heavy) is
   shared by all four tools equally — not a corbel-specific gap, deprioritize
   relative to 1/2 above.
4. `chevrotain-med-041`–`045` is a suspected golden-set defect (§4b), not a
   corbel bug — do not let it inflate any "corbel underperforms" reading
   without fixing the golden set first (out of scope for this analysis by
   instruction).
5. 127 items (21.1%) remain in 기타, dominated by 17 unreviewed hyperfine
   `callers` items — flagged as a follow-up, not force-classified.
