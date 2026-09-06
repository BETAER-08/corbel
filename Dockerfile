# syntax=docker/dockerfile:1

# corbel is an MCP server that requires a pre-built index before `corbel serve`
# will do anything useful (`corbel serve` refuses to start without one, see
# crates/corbel/src/commands/serve.rs). Glama's release pipeline builds this
# image, starts the server, and evaluates its tools (Server Coherence, Tool
# Definition Quality) without mounting any repository into the container. To
# give those checks real data to work with, this image indexes corbel's own
# source at build time (`corbel index`) and ships that index baked in, so
# `corbel serve` responds against real symbols out of the box. Point corbel at
# a different repo by mounting it over /app and re-running `corbel index`.

FROM rust:1.97.1-alpine AS builder

# musl-dev/gcc/g++/make: rusqlite's "bundled" feature and the tree-sitter
# grammars (rust/python/typescript/javascript) each compile a C (and, for
# some scanners, C++) source file via the `cc` crate.
RUN apk add --no-cache musl-dev gcc g++ make

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

ENV RUSTFLAGS="-C target-feature=+crt-static"
RUN cargo build --release --locked -p corbel --target x86_64-unknown-linux-musl

RUN cp target/x86_64-unknown-linux-musl/release/corbel /usr/local/bin/corbel \
    && /usr/local/bin/corbel index /app

FROM scratch

COPY --from=builder /usr/local/bin/corbel /usr/local/bin/corbel
COPY --from=builder /app/Cargo.toml /app/Cargo.lock /app/
COPY --from=builder /app/crates /app/crates
COPY --from=builder /app/.corbel /app/.corbel

WORKDIR /app
ENTRYPOINT ["/usr/local/bin/corbel"]
CMD ["serve", "/app"]
