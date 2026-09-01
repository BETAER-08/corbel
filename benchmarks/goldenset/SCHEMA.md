# Golden-set entry schema (confirmed)

This is the confirmed schema for `benchmarks/golden/*.json` entries, as
approved in the golden-set design review. It extends the format
`benchmarks/harness/run_benchmark.py` and `benchmarks/harness/metrics.py`
already read — it does not replace it. Existing fields (`id`, `symbol`,
`category`, `tasks.callers`, `tasks.callees`, `verification_note`) keep
their exact current meaning because `metrics.caller_key` /
`metrics.callee_key` / `run_benchmark.classify_miss` /
`run_benchmark.classify_extra` already depend on them by name and by
specific string value (`"overload_ambiguous_name"`, `"dynamic_dispatch"`,
`"multi_hop"`). This document only fixes the *new* fields and the rules
around them.

## Fields

```jsonc
{
  "id": "rust-hard-003",                 // {repo-slug}-{difficulty}-{seq:03d}
  "repo": "hyperfine",
  "commit": "f12f3d9f86f3643b3b7deace5e160b1f0f44d2b7",
  "language": "rust",
  "difficulty": "hard",                  // easy | medium | hard | adversarial
                                          // sampling/reporting only — run_benchmark.py
                                          // does not branch on this field
  "category": "overload_ambiguous_name", // existing field, unchanged meaning
  "symbol": { "name": "...", "file": "...", "line": 0, "kind": "...", "owner": null },
  "tasks": {
    "callers": [
      {
        "file": "...",
        "line": 0,               // caller's own enclosing-definition line (unchanged meaning)
        "enclosing_symbol": "...", // Rust "Owner::method" / Python "Owner.method" /
                                    // TS,TSX,JS "Owner.method" (corbel itself never
                                    // qualifies with owner in any of the five
                                    // languages — see corbel-lang/src/langs/*.rs
                                    // enclosing_definition_name — so this is always
                                    // the linguistically correct answer, not what
                                    // corbel returns; classify_miss already buckets
                                    // the gap as "unqualified_symbol_name" rather
                                    // than a genuine miss)
        "call_line": 0            // NEW. Actual call-site line. metrics.caller_key
                                   // is (enclosing_symbol, file) only — this field
                                   // is not read by any scoring code today. It exists
                                   // so a future call-site-aware corbel doesn't
                                   // require re-verifying every entry by hand.
      }
    ],
    "callees": [ /* unchanged */ ],
    "impact": null                // NEW. null | object. See "impact" below.
  },
  "verification": {
    "verified_by": "...",
    "verification_date": "YYYY-MM-DD",
    "verification_method": "...",
    "verification_note": "...",   // existing field, unchanged meaning
    "reverification": null        // NEW, adversarial-only. See "reverification" below.
  }
}
```

## `impact` (condition 1: null for easy/medium)

`tasks.impact` is `null` for every `easy` and `medium` entry. It is
required (non-null) for every `hard` and `adversarial` entry.

Rationale, as decided: no harness code scores `impact` yet — there is no
`run_impact_task` in `run_benchmark.py` and no `corbel_impact` adapter in
`tool_adapters.py` (confirmed absent by reading both files). Hand-tracing
a multi-hop chain for entries the scoring code cannot use yet is a sunk
cost anywhere it isn't already happening as a byproduct of verification.
For `hard`/`adversarial` entries the caller chain is already being
walked by hand to satisfy the C procedure (LSP + ripgrep cross-check,
corbel isolated), so capturing depth-2/3 callers at the same time is
close to free. For `easy`/`medium` it would be new, unnecessary work, so
it's skipped until a harness change to score `impact` is separately
approved.

When present, shape is unchanged from the design doc:

```jsonc
{
  "max_depth": 3,
  "affected": [
    { "depth": 1, "file": "...", "line": 0, "enclosing_symbol": "..." }
  ],
  "note": null   // fill in if depth 2/3 could not be exhaustively hand-traced
}
```

## `reverification` (condition 2: adversarial-only cross-check)

Every entry with `"difficulty": "adversarial"` must be verified twice, in
two separate sessions (not two passes in one sitting), before being
committed. The second session repeats the full C procedure from
scratch — LSP candidate collection, ripgrep cross-check, manual read —
without looking at the first session's `verification_note` until its own
verdict is written down.

`verification.reverification`:

```jsonc
{
  "verified_by": "...",
  "verification_date": "YYYY-MM-DD",
  "agrees_with_first_pass": true,   // false triggers mandatory human review,
                                     // not a silent pick of either answer
  "note": "..."                     // required when agrees_with_first_pass is false:
                                     // what differed and how it was resolved
}
```

`reverification` is `null` for every non-adversarial entry — it is not a
general-purpose QA field, it exists specifically because adversarial
entries are the ones published as corbel's public loss cases and need
the highest confidence.

## Validation

A `validate_schema.py` script (not yet written — build only if the pilot
shows manual review misses schema drift) would check:

- every `easy`/`medium` entry has `tasks.impact == null`
- every `hard`/`adversarial` entry has `tasks.impact != null` (or a
  `note` explaining why not)
- every `adversarial` entry has a non-null `verification.reverification`
- every golden-set *file* still carries the top-level
  `"corbel was never executed to produce any answer in this file"`
  attestation string
