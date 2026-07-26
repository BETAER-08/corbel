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

## Adding a language

A new language is promoted only when it clears every gate below:

- `extract_signature` and `is_public` are implemented for real, not left at a
  default that returns empty or trivial values.
- `build_scope` is implemented.
- The shared fixtures pass.
- The unresolved ratio stays at or below the threshold.

Pull requests that route around these gates with a default implementation are
not accepted.

## Merging

Pull requests are merged with squash merge only.
