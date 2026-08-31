# Contributing

## Development environment

The toolchain is pinned by `rust-toolchain.toml`. Running any cargo command in
this repository selects the correct compiler, rustfmt, and clippy automatically.

## Verification

Run all of the following before opening a pull request:

```
cargo fmt --all
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
```

## Developer Certificate of Origin

Every commit must be signed off under the
[Developer Certificate of Origin 1.1](https://developercertificate.org/).
Signing off is a statement that you wrote the change (or otherwise have the
right to submit it) and agree to submit it under this project's license — it
is not a copyright assignment and requires no separate paperwork.

Sign off with `-s` on every commit:

```
git commit -s -m "add TSX import-namespace test fixture"
```

This appends a trailer to the commit message:

```
Signed-off-by: Your Name <your.email@example.com>
```

The name and email must match the commit's author identity
(`git config user.name` / `user.email`), since that identity is what the
sign-off is attesting for.

If you forgot the flag:

- Last commit only: `git commit --amend -s --no-edit`
- Every commit on the branch since it diverged from `main`:
  `git rebase --signoff main`

CI checks every commit introduced by a pull request (not the repository's
existing history) for a matching `Signed-off-by` trailer and fails the
pull request if any commit is missing one.

## Adding a language

A new language is promoted only when it clears every gate below:

- `extract_signature` and `is_public` are implemented for real, not left at a
  default that returns empty or trivial values.
- `build_scope` is implemented.
- The shared fixtures pass.
- The unresolved ratio stays at or below the threshold.

Pull requests that route around these gates with a default implementation are
not accepted.

### Default implementation policy on `LanguageSupport`

A method that returns language-specific data — a symbol kind, a signature, a
visibility flag, a scope entry, or anything else whose correct value differs
per language — must never carry a default implementation. It must be
`required`, with no body, forcing every language to supply its own real
answer. This is a hard rule, not a style preference: amdb 1.0 shipped default
bodies that quietly returned plausible-looking placeholder data for
languages that hadn't implemented the method yet, and that fabricated data
was indexed and served as if it were real, silently corrupting query results
for months before anyone noticed.

A method that only orchestrates — calling other required methods and
assembling their results into a return value, without inventing or guessing
at any language-specific fact itself — may carry a default implementation.
`extract_symbols` and `extract_references` are the current examples: their
default bodies run the tree-sitter query returned by the required
`symbol_query`/`reference_query` methods and delegate every piece of
language-specific judgment (the symbol's kind, its signature, its
visibility) to the required methods that already exist for that purpose.
Sharing this orchestration avoids duplicating identical query-running
boilerplate across every language implementation, with no risk of a
language silently inheriting wrong data, because there is no data left for
the default body to get wrong.

When adding a method to `LanguageSupport`, ask: does this method decide any
fact about the language, or does it only combine facts decided elsewhere? If
it decides a fact, it is required. If it only combines, a default is
allowed.

## Changing the schema

Any change to `SCHEMA_DDL` in `crates/corbel-core/src/store/schema.rs` — a new
column, a new table, a changed `CHECK` constraint, anything that alters what
a row means — requires all of the following in the same pull request:

- Bump `CURRENT_SCHEMA_VERSION` in `schema.rs` by exactly 1.
- Add a `migrate_v{N-1}_to_v{N}` function in `store/migrate.rs` that
  transforms a database at the previous version into one at the new version,
  and add its `N-1 => migrate_v{N-1}_to_v{N}(conn)?` arm to the `migrate`
  loop's `match`.
- If the migration changes what gets stored for existing rows (not just adds
  an empty new column), invalidate `files.hash` for every row inside that
  migration function (`UPDATE files SET hash = '';`), the same way
  `migrate_v1_to_v2` and `migrate_v2_to_v3` already do. `files.hash` is a
  change-detection signal that is only valid relative to the exact
  parsing/extraction logic that produced the rows keyed by it; if you skip
  this, `corbel index` will see unchanged file content, trust the stale
  hash, and skip re-parsing files whose stored data no longer matches what
  the new schema expects.
- Add a test in `store_tests.rs` asserting the migration's data
  transformation, following `migrate_v1_to_v2_...`/`migrate_v2_to_v3_...`.

Pull requests that change `SCHEMA_DDL` without a matching migration step are
not accepted, even if the change looks additive.

### Why this is a hard rule

`corbel serve` opens whatever `.corbel/index.db` is already on disk and
answers queries against it — it has no way to know, from the response alone,
whether the schema it is reading matches the schema its own query code
assumes. Get this wrong and the result is not a crash, it is a wrong answer
returned with total confidence: a caller list that is quietly empty because
a migration step dropped and recreated a table without repopulating it, or a
query built against columns the running binary does not know about.

This class of bug was found in this repository during the schema-versioning
work itself, not hypothesized after the fact: `corbel serve` and
`corbel index` both called the same `open_connection`, which silently ran
the full forward migration chain on whatever version it found, including on
the `serve` (read) path — where a migration step drops and recreates
`relationships`/`imports` and leaves them empty until the next `corbel index`
run repopulates them. A user who only ran `serve` after upgrading corbel
would get an MCP server that returned confidently empty caller/callee lists
for real symbols, with no error at all. `open_for_serve`
(`store/migrate.rs`) exists specifically so that `serve` refuses to start on
anything other than an exact schema-version match, instead of guessing it
can paper over the difference; it opens the database read-only
(`OpenFlags::SQLITE_OPEN_READ_ONLY`, plus `PRAGMA query_only`) so that
"read-only" is enforced by the connection itself, not just by convention.

This is the same failure shape as the default-implementation problem
described above for `LanguageSupport`: code silently produced
plausible-looking data instead of admitting it did not have what it needed,
and that data was served as if it were real. amdb 1.0 did this with
per-language defaults; this repository nearly did it with a schema
mismatch. The fix in both cases is the same principle: when the code does
not have a real, version-matched answer, it must say so loudly, not paper
over the gap.

### Deciding what counts as a schema change

If you are not sure whether a change needs a migration, ask: could a
database file written by the *previous* `CURRENT_SCHEMA_VERSION` be queried
correctly by code written against the *new* one, unmodified? If yes (for
example, adding a covering index that changes nothing about query results,
only their speed), no migration is needed and the version stays the same.
If no — a new required column, a changed `CHECK` constraint, a renamed or
restructured table, a changed meaning for an existing column's values — it
is a schema change and needs the full migration treatment above.

## Merging

Pull requests are merged with squash merge only.
