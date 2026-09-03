# TypeScript/JavaScript regex definition matching — known gaps

`tool_adapters._ts_def_pattern` (used by `ripgrep_find_definition` /
`grep_find_definition`) and `enclosing._build_ts_index` (used by
`ripgrep_find_callers` / `grep_find_callers` to resolve which function a call
site is inside) are regex/indentation heuristics, not a TS parser. This is
the same tradeoff the existing Rust and Python patterns already make; this
file records where the TS/JS version specifically falls short, since its
definition shapes are more varied than `fn` / `def`.

## False negatives (a real definition is not matched)

- **Generic type parameters between the name and the parameter list.**
  `function foo<T>(x: T)` and `class Foo<T>` are not matched — the patterns
  require `(` (or, for classes, nothing but whitespace) immediately after the
  name.
- **Multi-line signatures.** If the opening `(` (function declarations) or
  the `=>` / `function` keyword (arrow/function-expression assignment) is not
  on the same source line as the name, the line-by-line regex scan misses it.
- **Generic arrow functions.** `const foo = <T,>(x: T) => {}` is not matched
  by the assigned-function pattern, which expects `(` or `function` right
  after `=`.
- **Object property shorthand definitions using computed keys** (`[key]:
  function() {}` or `[key]() {}`) are not matched — the patterns require a
  literal identifier name.
- **Allman-style braces.** The class/object-method pattern requires the
  opening `{` on the same line as the signature (`method(x) {`), to avoid the
  false-positive problem below. `method(x)\n{` on its own line is not
  matched.

## False positives (something that is not a definition gets matched)

- **Interface/type method signatures.** `interface Foo { bar(x: number):
  void }` has no implementation, but the class/object-method pattern matches
  `bar(` as if it were one.
- **Plain call expressions at the start of a line** (e.g. a call left
  unassigned as a statement, `doThing(x);`) could match the class/object-method
  pattern, which has no keyword anchor and only filters by keyword-list
  exclusion (`TS_CONTROL_KEYWORDS` / `TS_JS_KEYWORDS`), not by syntactic
  position — mitigated (not eliminated) by requiring a trailing `{` on the
  same line, since a call statement ends in `;`/nothing, not `{`. A call
  immediately followed by a same-line block (rare, but e.g. `doThing(x) {`
  inside some macro-like DSL) would still false-positive.
- **Overloaded function declarations.** TypeScript overload signatures (`function
  foo(x: string): void; function foo(x: number): void; function foo(x) {
  ... }`) each match independently, so `ripgrep_find_definition` /
  `grep_find_definition` report multiple hits for a name that has one real
  implementation.

## Scope/owner attribution (`enclosing.py`)

- Owner (class) attribution is purely indentation-based, identical in spirit
  to the existing Python heuristic: a definition is attributed to the
  nearest preceding `class` line with strictly smaller indentation. Deeply
  nested closures, IIFEs, or a function whose body happens to dedent to the
  same column as an enclosing class can be mis-attributed the same way the
  Python heuristic can.
- Interfaces are not tracked as owners at all (`TS_CLASS_RE` only matches
  `class`), so interface method signatures — even where they are not
  filtered out as false positives above — never get a qualified owner name.
- **Nested local closures are deliberately excluded from the enclosing
  index.** `const x = function() {}` / arrow-assignment matches are only
  registered when they sit at indent 0 (module scope). The index has no
  concept of where a definition's body ends, so a nested closure (e.g. a
  helper assigned inside another function's body, a very common TS/JS
  pattern) would otherwise become the "last preceding definition" and
  wrongly swallow enclosing-symbol attribution for every line after it,
  including lines that are actually still inside the *outer* function, not
  the closure. The tradeoff: a call site whose true enclosing scope *is* a
  nested closure is instead attributed to the nearest enclosing top-level
  function/class member.

## Also worth knowing: ctags' own TypeScript gaps

Universal Ctags' bundled TypeScript grammar independently fails to tag some
patterns this harness's regex *does* catch — e.g. a function expression
assigned to an `export const` was observed missing from ctags' output on the
chevrotain golden set. `ripgrep+ctags`'s callers/callees tasks bypass this by
using ripgrep for call-site discovery rather than the ctags tag table, but
its **definition** task (`ctags_find_definition`, pure tag lookup with no
regex fallback) inherits ctags' TS grammar gaps directly.
