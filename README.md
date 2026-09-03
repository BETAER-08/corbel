# corbel

corbel is a local **MCP server** that performs **static analysis** to build a resolved **call graph** of your codebase, so a coding agent can ask "what calls this?" and "what breaks if I change this?" without guessing. It is not AI-based — no model runs inside it, and it makes no probabilistic claims about your code. corbel is also the name of an architectural bracket, and unrelated to the Microsoft font of the same name.

*A corbel (/ˈkɔːrbəl/) is the bracket built into a wall that carries the load above it. corbel maps what carries what in your code.*

corbel ships as a single static binary with no runtime dependencies, and your code never leaves the machine.

## The problem, in one real query

"What calls `format_duration_unit`?" — ripgrep and corbel, run against [sharkdp/hyperfine](https://github.com/sharkdp/hyperfine) at `f12f3d9f` (pinned so these numbers don't drift — clone it yourself to reproduce):

```
$ rg -n 'format_duration_unit\(' src/
src/output/format.rs:6:    let (duration_fmt, _) = format_duration_unit(duration, unit);
src/output/format.rs:11:pub fn format_duration_unit(duration: Second, unit: Option<Unit>) -> (String, Unit) {
src/output/format.rs:30:    let (out_str, out_unit) = format_duration_unit(1.3, None);
src/output/format.rs:35:    let (out_str, out_unit) = format_duration_unit(1.0, None);
...8 more lines, each a bare file:line with no indication of which function the call is inside
```

ripgrep finds every text occurrence of `format_duration_unit(` — 12 lines (the declaration plus 11 real calls), unlabeled. Telling which caller is which, and which are duplicates from the same test, means opening the file and counting by hand. corbel's `get_symbol`, called on the same function, resolves each hit to the function it's actually inside:

```json
{
  "callers": [
    { "file": "src/benchmark/mod.rs", "line": 141, "name": "Benchmark::run", "resolution": "scoped" },
    { "file": "src/output/format.rs", "line": 5, "name": "format_duration", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 29, "name": "test_format_duration_unit_basic", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 62, "name": "test_format_duration_unit_with_unit", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 62, "name": "test_format_duration_unit_with_unit", "resolution": "same-file" },
    { "file": "src/output/format.rs", "line": 62, "name": "test_format_duration_unit_with_unit", "resolution": "same-file" }
  ]
}
```

Six of those 11 calls are inside `test_format_duration_unit_basic` (one assertion per call), three inside `test_format_duration_unit_with_unit` — `get_symbol` tells you that directly; grep leaves you to work it out by reading the file. That's the gap corbel closes: not finding text, but naming the caller. This exact case (`hyperfine-10` in the golden set below) is hand-verified — corbel's answer here matches ground truth exactly, precision and recall both 1.0.

## Install

```
cargo install corbel
```

Pre-built binaries are produced by [cargo-dist](https://github.com/axodotdev/cargo-dist) shell and PowerShell installers on tagged releases:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/BETAER-08/corbel/releases/latest/download/corbel-installer.sh | sh
```

```powershell
powershell -ExecutionPolicy ByPass -c "irm https://github.com/BETAER-08/corbel/releases/latest/download/corbel-installer.ps1 | iex"
```

**Supported platforms** (per `dist-workspace.toml`, each built and tested in CI): `aarch64-apple-darwin`, `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.

## Claude Code, in three steps

```
corbel index .
claude mcp add corbel -- corbel serve
```

Then ask a refactoring question in plain language — the agent calls `get_symbol`/`impact`/`find` on its own:

> "If I change `resolve_all`, what else needs to change?"

For other MCP clients, add corbel directly to the server config:

```json
{
  "mcpServers": {
    "corbel": { "command": "corbel", "args": ["serve"] }
  }
}
```

## The three tools

Examples below are all real responses against the same pinned repo as above ([sharkdp/hyperfine](https://github.com/sharkdp/hyperfine) at `f12f3d9f`) — clone it and run these yourself to check.

**`get_symbol`** looks up a symbol by name and returns its definition (file, line, signature) plus everything that calls it and everything it calls. Every edge carries a `resolution` field explaining how corbel matched it to a specific definition. Real response, `get_symbol("format_duration_unit")`:

```json
{
  "results": [{
    "name": "format_duration_unit",
    "file": "src/output/format.rs",
    "line": 11,
    "signature": "pub fn format_duration_unit(duration: Second, unit: Option<Unit>) -> (String, Unit)",
    "callers": [
      { "file": "src/benchmark/mod.rs", "line": 141, "name": "Benchmark::run", "resolution": "scoped" }
      /* ...10 more, see above */
    ],
    "callees": [
      { "file": "src/output/format.rs", "name": "format_duration_value", "resolution": "same-file" }
    ],
    "truncated": false
  }]
}
```

**`impact`** is the flagship tool: it walks the reverse call graph from a symbol across multiple hops and returns every affected symbol tagged with `depth` and `resolution` — the multi-hop trace a single grep or a one-hop "find references" cannot do. Real response, `impact("compute_relative_speeds")` (6 affected symbols total):

```json
{
  "results": [{
    "target_name": "compute_relative_speeds",
    "affected": [
      { "depth": 1, "file": "src/benchmark/relative_speed.rs", "line": 86, "name": "compute_with_check_from_reference", "resolution": "same-file" },
      { "depth": 1, "file": "src/benchmark/relative_speed.rs", "line": 98, "name": "compute_with_check", "resolution": "same-file" },
      { "depth": 2, "file": "src/benchmark/relative_speed.rs", "line": 143, "name": "test_compute_relative_speed", "resolution": "same-file" }
    ],
    "affected_count": 6,
    "max_depth_reached": 2,
    "truncated": false
  }]
}
```

**`find`** is a name search over the index, for when the exact name to hand `get_symbol` isn't known yet. It does not resolve call relationships. Real response, `find("duration", limit=3)` — 5 symbols match, 3 are returned:

```json
{
  "results": [
    { "name": "format_duration", "file": "src/output/format.rs", "line": 5, "kind": "function" },
    { "name": "format_duration_unit", "file": "src/output/format.rs", "line": 11, "kind": "function" },
    { "name": "format_duration_value", "file": "src/output/format.rs", "line": 18, "kind": "function" }
  ],
  "total_matches": 5,
  "truncated": true,
  "truncated_count": 2
}
```

## Supported languages

| Language | Level | Notes |
| --- | --- | --- |
| Rust | full | Own scope walker; all five resolution stages exercised. |
| Python | full | Own scope walker; all five resolution stages exercised. |
| TypeScript | full | Own scope walker; all five resolution stages exercised. |
| TSX | full | Adds JSX-tag references on top of TypeScript's resolution. |
| JavaScript | full | Shares TypeScript's resolution machinery. CommonJS `require(...)` produces no import entry — only ES-module `import`/`export` is scope-aware. |

Every language above runs through the same five-stage resolution chain, implemented once and shared by all of them: **same-file → scoped → global-unique → external → unresolved**. See [docs/language-support.md](docs/language-support.md) for the promotion criteria new languages must clear.

## Measured accuracy

Text search doesn't just miss things — it over-reports, confidently. A real, reproducible example from this benchmark's own repos:

```
$ rg -n '\.iter\(' --type rust src/    # inside hyperfine's own source tree
31 matches
```

Only one of those 31 hits is a call to the specific `Commands::iter` method a caller-graph query is actually asking about; the other 30 are `.iter()` on unrelated `Vec`s and slices — one of the most common method names in any Rust codebase. That gap between "what text search finds" and "what's actually being asked" is what a resolved call graph closes, and why we score it rather than just describe it.

Measured against a 120-entry hand-verified golden set (callers + definition tasks; see [Methodology](#methodology)), precision / recall / F1, split by language rather than averaged away:

| Language | corbel | grep / ripgrep¹ | ripgrep+ctags² |
| --- | --- | --- | --- |
| TypeScript | 0.868 / 0.820 / 0.844 | 0.387 / 0.424 / 0.404 | 0.783 / 0.880 / 0.829 |
| Rust | 0.586 / 0.488 / 0.532 | 0.512 / 0.714 / 0.597 | 0.530 / 0.739 / 0.617 |
| Python | 0.618 / 0.920 / 0.740 | 0.524 / 1.000 / 0.688 | 0.524 / 1.000 / 0.688 |
| **Overall** | **0.709 / 0.705 / 0.707** | 0.472 / 0.640 / 0.543 | **0.617 / 0.844 / 0.713** |

*(precision / recall / F1)*

**corbel loses to `ripgrep+ctags` overall** (F1 0.707 vs 0.713) and on Rust specifically (0.532 vs 0.617) — left in the table as measured.

¹ `grep` and `ripgrep` score byte-identically here — shown as one column; see Reproducibility below.
² a hybrid, not plain ctags: ripgrep finds call sites, ctags supplies the enclosing scope for each hit. Plain ctags has no call-site index, so the callers task is structurally impossible for it alone — this scores the hybrid a real developer would actually reach for, not a strawman zero.

Scoring caveats, disclosed rather than tuned away:
- T2 (callees) is excluded from this table — ~90% of its golden-set ground truth is empty, so scoring it would grade "did you correctly return nothing," not tool capability.
- The automatic classifier used to categorize corbel's misses doesn't account for call-count multiplicity: corbel's `callers` list is one row per caller *symbol*, not one row per call site, so a symbol calling the target twice scores as a miss even when corbel names the right function. 18 of 19 failures this classifier tags `unqualified_symbol_name` are this artifact, not a remaining qualification bug (one of the 19 is real — see Known limitations). This was not changed after seeing what it produced.

Full per-entry breakdown and adversarial-case detail: [benchmarks/results/](benchmarks/results/).

### Reproducibility

grep and ripgrep's numbers above are **byte-identical** to a run taken before the fix that moved corbel's own F1 from 0.395 to 0.707 — direct evidence the harness and golden set were not adjusted to move corbel's number:

- [before](benchmarks/results/benchmark-20260903T092406Z.json) — corbel F1 0.395 ([analysis](benchmarks/results/benchmark-20260903T092406Z-analysis.md))
- [after](benchmarks/results/benchmark-20260903T142000Z.json) — corbel F1 0.707 ([analysis](benchmarks/results/benchmark-20260903T142000Z-analysis.md))

```
python3 benchmarks/harness/run_benchmark.py
```

## Performance at scale

Measured on real open-source repositories, not accuracy-scored. Full methodology: [benchmarks/results/perf-20260904.md](benchmarks/results/perf-20260904.md).

| Symbols | Repo | Cold index | `find` p50 / p99 |
| --- | --- | --- | --- |
| 8,145 | tokio | 9.6s | 1.1 / 1.4ms |
| 31,849 | bevy | 27.7s | 4.9 / 5.8ms |
| 112,940 | TypeScript compiler | 282s | 15.0 / 16.9ms |
| 116,870 | servo | 566s | 31.7 / 122ms |

- **Cold-index time is super-linear** (exponent ≈2.3 between the 32K and 110K+ tiers). The driver isn't symbol count: servo and the TypeScript compiler have almost the same symbol count, but servo takes 2x longer because it has 5.8x more name-collision call sites (164,043 vs 28,282) — the bare-name resolution lookup, not indexing itself, is the bottleneck.
- **`find` runs two full-table scans per call** (`LIKE '%query%'`, a leading wildcard that can't use the name index) — negligible under ~32K symbols, 15-32ms typical past 110K, p99 up to 122ms. The threshold that matters isn't a symbol count, it's call frequency: a workflow issuing several `find` calls per task feels this before any single call does.
- **Peak memory stays flat** (8.4-12.3MB) regardless of repo size — time and tail latency are the constraint, not memory.
- `impact` has no depth parameter (always walks to depth 10 or budget exhaustion) — depth-3-specific latency wasn't measurable and is reported as such, not approximated.
- Only one cold-index run, not three, at the 100K+ tier — a single run cost 9-10 minutes, making repeated averaging impractical in itself. `rust-lang/rust` was not attempted.

## Methodology

corbel's own output was never used to build the golden set — `candidate_scanner.py`, which selects candidate symbols, doesn't import corbel; this is a structural guarantee, not a policy. Every one of 120 entries was verified by a single AI model (Claude Sonnet 5) reading source directly, cross-checked against ripgrep-enumerated candidates and an LSP server's draft answer — **there is no human review of individual entries**, disclosed here because it matters, not because it's flattering. The LSP cross-check itself produced 6 distinct classes of wrong answers, catalogued rather than trusted blindly: [LSP_ERROR_TYPES.md](benchmarks/goldenset/LSP_ERROR_TYPES.md). Text-search overcounting (the `.iter(` example above and others, up to 31x) is catalogued the same way: [TEXT_SEARCH_LIMITATIONS.md](benchmarks/goldenset/TEXT_SEARCH_LIMITATIONS.md). The 12 hardest (`adversarial`) entries got a second, independently-derived verification pass from a context-isolated subagent that never saw the first pass's reasoning.

Full methodology, including what the single-verifier limitation does and doesn't compensate for: [benchmarks/README.md](benchmarks/README.md).

## Known limitations

corbel resolves what static analysis can prove and refuses to guess at the rest. On its own source (796 symbols, 5481 references at time of writing), 93.3% of internal calls resolve; the remaining 6.7% are calls where more than one definition shares a name and nothing in scope disambiguates them, so corbel marks them `unresolved (ambiguous)` rather than picking one.

Cases that are structurally out of reach for static analysis, by design, in every supported language:

- **Dynamic dispatch** — trait objects (Rust), duck-typed calls (Python), calls through an interface-typed value (TypeScript) — has no statically-determined target. corbel reports these as `external` or `unresolved`, never a fabricated edge.
- **Macro-generated code** (Rust `macro_rules!`/derive output, decorators that rewrite call sites) is invisible to corbel's tree-sitter-based extraction if the macro expansion isn't present in source form. Checked, not just claimed: across every measured failure in the golden set's three repos, **zero** were traced to a macro-generated call site — a confirmed absence, not a category we didn't look for.
- **JavaScript/TypeScript CommonJS `require(...)`** doesn't populate an import entry, so a call reached only through a `require`-bound name can resolve less precisely than the same call reached through `import`.
- **`find`'s substring query** (`%query%`) cannot use the symbol-name index — every call does a full scan of the `symbols` table. Negligible under ~32K symbols, 15-32ms typical past 110K (see Performance above).
- **Standard library and external crate/package calls** are outside the index entirely and reported as `external` — corbel does not resolve into dependencies.

**Measured breakdown of corbel's actual misses** (603 classified failures, callers + definition tasks): 13.4% name collision (a bare-name lookup limitation shared by every tool measured here, not corbel-specific), 6.8% dynamic dispatch, 2.3% a runtime prototype-assembly pattern (`chevrotain`'s `applyMixins`, which does `derivedCtor.prototype[key] = baseCtor.prototype[key]` at runtime — invisible to static analysis by construction), 0.8% duck typing. Full breakdown: [analysis](benchmarks/results/benchmark-20260903T142000Z-analysis.md).

Name collision, concretely: [pallets/itsdangerous](https://github.com/pallets/itsdangerous) at `672971d6` defines `__init__` 13 times across its class hierarchy. `get_symbol("__init__")` with no `file`/`line` to disambiguate returns all 13 — corbel narrows by name, not by which class you meant, same as any bare-name index would:

```
$ corbel index . && echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_symbol","arguments":{"name":"__init__"}}}' | corbel serve .
# 13 results across src/itsdangerous/exc.py, serializer.py, signer.py
```

Pass `file` (and `line`, if the file still has more than one match) — exactly what `find`'s results give you — to get exactly one.

**A real, unfixed bug**: a Rust method defined as a trait's *default method body* (`trait Foo { fn bar(&self) { ... } }`, not inside an `impl` block) doesn't get an owner-qualified caller name — `owner_of_definition` walks up looking for `impl_item` and never checks for an enclosing `trait_item`. 3 occurrences in the benchmark (`MarkupExporter::table_results` in hyperfine). Narrow, but real, and listed here rather than folded into the percentages above.

**Cases where every tool measured — corbel, grep, ripgrep, and the ripgrep+ctags hybrid — gets the same answer wrong:**
- `for x in iter` desugars to repeated `Iterator::next()` calls with no `.next()` text anywhere in source. No tool here does implicit-desugaring analysis; all four fail identically — a shared ceiling, not a corbel gap.
- Five `chevrotain` entries (`findEndOfInputAnchor` and four siblings) appear to have an incorrect golden-set answer: their real sole caller is `validateRegExpPattern`, but the golden set records `validatePatterns` (one level further out). All four tools agree on `validateRegExpPattern` and are uniformly scored wrong against it. **This was not corrected in the golden set** — the entries stand as originally verified, flagged here instead, so a scoring artifact doesn't get fixed quietly after the fact.

## License and boundaries

corbel is licensed under [MIT](LICENSE).

Indexing and querying your own codebase — the entire tool as it exists today — is and will remain free for individual use, with no license server, no telemetry, and no phone-home behavior, ever. Organization-level features (fleet-wide indexing, shared indexes, team administration) are the intended boundary for a future commercial offering; nothing in the current codebase is gated, and this line is drawn now, before any such feature exists, rather than moved after the fact.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the development workflow, the language-promotion gates, and the schema-migration rules. Every commit must carry a DCO sign-off (`git commit -s`).

## Privacy

corbel never sends your code anywhere. Indexing and querying run entirely offline; the binary contains no network code. What an agent sends to its model is between the agent and its MCP client — corbel itself never touches the network.

## Non-goals

corbel does not edit code, generate documentation, ship a web UI, read git history, scan for secrets, integrate with the Language Server Protocol, or collect telemetry.
