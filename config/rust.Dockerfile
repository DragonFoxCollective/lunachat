# Build stage
FROM rust:1.95 AS builder

WORKDIR /app

COPY lunachat/src /app/src
COPY lunachat/templates /app/templates
COPY lunachat/Cargo.toml /app/Cargo.toml
COPY Cargo.lock /app/Cargo.lock

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/main /usr/local/bin/lunachat


# Runtime stage
FROM debian:bookworm

WORKDIR /app

COPY --from=builder /usr/local/bin/lunachat /usr/local/bin/

COPY lunachat/static /app/static

CMD ["sh", "-c", "/usr/local/bin/lunachat"]
