# syntax=docker/dockerfile:1.7

FROM rust:1.97-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends libssl-dev pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src/lib.rs ./src/lib.rs
RUN cargo fetch --locked

COPY build.rs ./build.rs
COPY migrations ./migrations
COPY src ./src

RUN --mount=type=cache,id=helt-blog-backend-release,target=/app/target \
    cargo build --locked --release \
    && cp /app/target/release/blog-backend /usr/local/bin/blog-backend-build

FROM builder AS test-base

COPY tests ./tests
RUN rustup component add clippy rustfmt

FROM test-base AS test

RUN --mount=type=cache,id=helt-blog-backend-test,target=/app/target \
    cargo fmt --all -- --check \
    && cargo test --locked --all-targets \
    && cargo clippy --locked --all-targets -- -D warnings

FROM test-base AS integration-builder

RUN --mount=type=cache,id=helt-blog-backend-integration,target=/app/target \
    cargo test --locked --test postgres_integration --no-run \
    && test_binary="$(find /app/target/debug/deps -maxdepth 1 -type f -name 'postgres_integration-*' -perm /111 -print -quit)" \
    && test -n "${test_binary}" \
    && cp "${test_binary}" /usr/local/bin/blog-integration-test

FROM debian:bookworm-slim AS runtime-base

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 blog \
    && useradd --uid 10001 --gid blog --no-create-home --home-dir /nonexistent blog

FROM runtime-base AS integration-test

COPY --from=integration-builder /usr/local/bin/blog-integration-test /usr/local/bin/blog-integration-test
USER blog:blog
ENTRYPOINT ["/usr/local/bin/blog-integration-test"]
CMD ["--ignored", "--test-threads=1"]

FROM runtime-base AS runtime

COPY --from=builder /usr/local/bin/blog-backend-build /usr/local/bin/blog-backend
RUN ln -s /usr/local/bin/blog-backend /usr/local/bin/blog-admin

USER blog:blog
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/blog-backend"]
