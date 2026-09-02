# Text-search overcounting, observed while building the golden set

Every golden-set entry starts from a `ripgrep` candidate scan
(`candidate_scanner.py`) and every caller/callee list is cross-checked
against `rg` output before being accepted (see `SCHEMA.md`'s "Verifier
independence" section). Because that cross-check means every raw `rg`
match gets individually read and classified as real-or-noise, the golden
set incidentally accumulates hard numbers on how badly a bare
name-matching text search overcounts a symbol's true caller set — this is
useful evidence independent of, and cheaper to cite than, a full benchmark
run: it doesn't require building corbel or running the harness, just
reading the `verification_note` fields already committed.

This file collects the clearest, most-quantified cases. All numbers are
taken verbatim from the cited entry's `verification_note`; none are
recomputed or estimated here.

## The overcounting cases

| Entry | Symbol | Raw text-search hits | Real callers | Ratio | Why the gap |
|---|---|---|---|---|---|
| `hyperfine-20` | `Commands::iter` | 31 (`rg -n "\.iter\(\)"`) | 1 | 31x | Virtually every hit is `.iter()` on an unrelated `Vec`/slice — `.iter()` is one of the most common method names in any Rust codebase. |
| `hyperfine-11` | `Benchmark::run` | 21 (`rg -n "\.run\("`) | 1 | 21x | `.run(` collides with every other type's own `run` method in the crate; none of the other 20 hits construct or hold a `Benchmark`. |
| `hyperfine-31` | (private `max` helper) | not stated exactly, described as "heavily" over-matched | 6 | not quantified | `rg -n "\bmax\("` also matches `f64::max` method calls (`.max(0.0)`) and `cmp::max(...)` from the standard library — same identifier, unrelated functions. |
| `hyperfine-18` | `Command::get_command` | 3 (`rg -n "get_command\("`, src/ + tests/) | 1 | 3x | 2 of the 3 hits are a same-named private method on an unrelated test-only fixture struct (`LoggedCommand`-style, `tests/execution_order_tests.rs:34`), nothing to do with `command.rs`'s `Command`. |
| `hyperfine-29` | `format_duration` | not separately counted; noted as inflated by name-prefix collision | 7 | not quantified | `format_duration_unit` and `format_duration_value` both share the `format_duration` text prefix — a substring/prefix search (or a careless regex) conflates 3 distinct functions. |
| `itsdangerous-14` | `is_text_serializer` | declaration + 2 real calls + 2 more | 2 | n/a (false-positive kind, not pure volume) | The 2 extra hits are reads of a same-named *instance attribute* (`self.is_text_serializer`, `serializer.py:254,317`) — a text search cannot distinguish an attribute read from a function call by name alone. |

## A case in the opposite direction: total invisibility, not overcounting

`hyperfine-39` (`RangeStep::next`, an `Iterator` impl) is the mirror image
of the table above: `rg -n "\.next\(\)"` finds 7 hits in the crate, and
**zero** of them are the real call. The one real invocation
(`src/parameter/range_step.rs`'s `Iterator::next`, called once in
production code at `src/command.rs:282`) is reached through
`for value in param_range { ... }`, which the Rust compiler desugars to
`while let Some(value) = param_range.next() { ... }` — a call with no
literal `.next()` text anywhere in the source for a text search to find.
This isn't a volume problem (text search doesn't overcount here — it
undercounts, returning 0 real matches out of 7), so it doesn't fit the
"raw hits vs. real callers" table above, but it's the same underlying
fragility from the opposite direction: name-based text search's accuracy
depends entirely on the call syntax matching a literal string, and neither
direction (over- or under-counting) is something the tool can self-detect.

## How to cite this

Every ratio above is reproducible by anyone with the cloned repo and
`rg` installed — re-run the quoted `rg` command against the pinned commit
in the corresponding `benchmarks/golden/<repo>.json`'s top-level `commit`
field, and count. This file exists so that claim doesn't have to be
re-derived from scratch (or worse, asserted without a number) every time
it comes up — e.g. in `benchmarks/README.md`'s comparison-against-grep
framing.
