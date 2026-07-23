FROM rust:1.97-slim-bookworm AS builder

WORKDIR /app
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src/lib.rs ./src/lib.rs
RUN cargo fetch --locked

COPY migrations ./migrations
COPY src ./src

RUN cargo build --locked --release

FROM builder AS test

RUN rustup component add clippy \
    && cargo test --locked --all-targets \
    && cargo clippy --locked --all-targets -- -D warnings

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 blog \
    && useradd --uid 10001 --gid blog --no-create-home --home-dir /nonexistent blog

COPY --from=builder /app/target/release/blog-backend /usr/local/bin/blog-backend
RUN ln -s /usr/local/bin/blog-backend /usr/local/bin/blog-admin

USER blog:blog
EXPOSE 3000
ENTRYPOINT ["/usr/local/bin/blog-backend"]
