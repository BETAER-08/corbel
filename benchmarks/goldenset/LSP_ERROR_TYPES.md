# LSP draft errors observed while building the golden set

`benchmarks/goldenset/lsp_to_draft.py` produces **unverified** draft entries
from an LSP server's `textDocument/references` (and `documentSymbol`)
responses. Every draft is subsequently cross-checked against `rg` output and
the source is read directly before anything is committed to
`benchmarks/golden/*.json` — this file exists because that cross-check step
is not a formality: across 3 languages / 3 LSP servers, drafts contained
real, reproducible errors that a naive "trust the LSP" pipeline would have
committed as ground truth.

This is a standing catalogue, updated as new golden-set work turns up new
failure shapes. It is intentionally kept separate from the golden-set JSON
files themselves (per the golden-set design review) since it describes
*tooling* behavior, not any one repo's ground truth.

## Error types found

### 1. Assignment/reference misread as a call

An LSP correctly resolves a reference to the right symbol, right file, right
line — but the reference is a **value use** (an assignment, a property
read, a function passed as a bare value), not a call expression. The draft
reports it as a "caller" anyway.

- **TypeScript / typescript-language-server**: `defaultVisit`
  (`packages/chevrotain/src/parse/cst/cst_visitor.ts:4`, chevrotain).
  `withDefaultsProto[ruleName] = defaultVisit;` (line 90) is a plain
  property assignment; the LSP draft reported it as a caller. `defaultVisit`
  has zero real call-syntax callers in the repository — it's invoked only
  via `this[name](...)` dynamic dispatch elsewhere.

### 2. Import statement misread as a call

A `references` response for symbol `S` includes the line of a
`import { S } from "..."` (or `import { S as T }`) statement in a file that
imports `S`, even though that line contains no call expression at all.

- **TypeScript**: `validateGrammar` (checks.ts:60, chevrotain) — 2 of 5
  reported references were `import { validateGrammar } from
  "../grammar/checks.js"`-style import lines in gast_resolver_public.ts,
  not calls.
- **TypeScript**: `resolveGrammar` (resolver.ts:11, chevrotain) — same
  pattern, 2 of 3 reported references were the aliased import line
  (`import { resolveGrammar as orgResolveGrammar } from "../resolver.js"`).

### 3. Missed enclosing-symbol attribution (returns null / wrong scope)

The LSP finds the correct call *line* but either returns no owning
function/method at all, or attributes the call to the wrong enclosing
scope because its notion of "enclosing" doesn't match the golden-set
schema's tree-sitter-based definition (walk up to the nearest
`function_declaration` / `method_definition`, or a `variable_declarator`
whose value is an arrow function — anonymous callback arguments don't
count as a scope boundary and are skipped).

- **TypeScript**: `getExtraProductionArgument` (checks.ts:149, chevrotain)
  — call at checks.ts:131 correctly located, enclosing_symbol returned as
  `null`. Manually attributed to `validateDuplicateProductions`.
- **TypeScript**: `validateAmbiguousAlternationAlternatives`
  (checks.ts:389, chevrotain) — the one real call (llk_lookahead.ts:86) was
  labeled `"validateAmbiguousAlternationAlternatives.rules.flatMap()
  callback"` (the innermost anonymous arrow passed to `.flatMap`), not the
  containing method — the draft's climbing rule differs from
  corbel-lang's own `enclosing_definition_name`, which skips
  non-variable-bound arrow arguments.
- **TypeScript**: `resolveGrammar` (gast_resolver_public.ts:19, chevrotain)
  — the one real call (parser.ts:173) was labeled
  `"TRACE_INIT(\"performSelfAnalysis\") callback.TRACE_INIT(\"Grammar
  Resolving\") callback"`, again the nested anonymous callback rather than
  the containing method (`Parser.performSelfAnalysis`).

### 4. Outright 0-of-N miss (real caller never surfaced)

The LSP's references response omits a real, unambiguous call site
entirely — every reported reference is noise (import lines, unrelated
files) and the actual call is simply absent from the result set.

- **Rust / rust-analyzer**: a function with 6 known real callers returned
  0 correctly-identified callers in one hyperfine-set draft (see
  hyperfine's own verification notes for the specific symbol).
- **TypeScript / typescript-language-server**: `validateGrammar`
  (gast_resolver_public.ts:34, chevrotain) — both reported references were
  the import line in parser.ts; the real call at parser.ts:183 was
  entirely absent from the draft.

### 5. False reference pointing to an unrelated symbol/file

A returned reference resolves to the right *name* but the wrong *symbol* —
e.g. a same-named method on an unrelated class, surfaced because the LSP's
resolution was structurally/type-looser than the golden set requires.

- **Rust / rust-analyzer**: a reference set for one method included a call
  through a differently-typed receiver whose static type did not match the
  target impl at all (see hyperfine set for the specific case).
- **TypeScript / typescript-language-server**: `reset`
  (`parse/parser/traits/looksahead.ts:207`, chevrotain) — draft references
  included `collectorVisitor.reset()` calls (looksahead.ts:254, 258) as
  callers of `RecognizerEngine.reset` (recognizer_engine.ts:873, the
  symbol actually mixed into `Parser` via `applyMixins`). `collectorVisitor`
  is an instance of an unrelated private class, `DslMethodsCollectorVisitor`,
  which defines its *own* unrelated `reset()` method — the two `reset`
  methods share a name and nothing else. This same investigation is also
  what disproved an initially-planned adversarial entry (a supposed
  `LooksAhead.reset` mixin collision) — the ctags-based candidate scanner
  flags name collisions mechanically, but only reading the source revealed
  the second `reset` had nothing to do with the mixin.

### 6. Python / pyright — type-directed false positive (found while cross-verifying, not from a `references` draft)

Not from `lsp_to_draft.py`'s `references` query, but from the same
"don't trust the tool blindly" discipline applied to pyright's static type
narrowing during itsdangerous verification: pyright statically attributes
`self.make_signer(salt).sign(payload)` (serializer.py:315) to
`Signer.sign` because `make_signer`'s declared return type is `Signer` —
but `make_signer`'s actual runtime behavior returns `self.signer(...)`,
where `self.signer: type[Signer]` is a class attribute overridden by
`TimedSerializer` to `TimestampSigner` (with an explicit
`# pyright: ignore` at the override site, i.e. pyright itself flags the
narrowing as unsound). A tool that trusts pyright's static type here will
silently miss that the same call site can resolve to
`TimestampSigner.sign` instead. See itsdangerous-25's verification_note.

## Cumulative count by language

| Language   | LSP server                    | Error types observed |
|------------|--------------------------------|----------------------|
| Rust       | rust-analyzer                  | #3 (partial-miss inside a long function), #4, #5 |
| Python     | pyright                        | #6 (type-directed false positive, distinct in kind from the reference-based errors above) |
| TypeScript | typescript-language-server     | #1, #2, #3, #4, #5 |

All 5 reference-based error types (#1–#5) were reproduced in TypeScript,
matching (and in the case of #1/#2, adding to) the set already known from
Rust. This is read as evidence that these are failure modes of the
"references + heuristic enclosing-scope climb" approach generally, not of
any one language server's implementation quirks.

## Practical implication for golden-set construction

Every entry in `benchmarks/golden/*.json` is built by: (1) generating a
draft via `lsp_to_draft.py`, (2) cross-checking every draft caller/callee
line against an independent `rg` search, (3) opening and reading the actual
source around every surviving candidate line, and (4) dropping — not
"fixing by guessing" — any candidate that can't be confirmed this way. The
error catalogue above is why step (2)–(3) are treated as mandatory rather
than spot checks: in this project's experience, a meaningful fraction of
LSP-drafted caller/callee rows for any given entry are wrong in one of the
above ways.
