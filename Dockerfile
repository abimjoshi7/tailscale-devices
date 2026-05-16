# ── Stage 1: build ────────────────────────────────────────────────────────────
FROM rust:1-slim-bookworm AS builder

WORKDIR /build

# Cache dependency compilation separately from source compilation
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release \
    && rm -f target/release/deps/tailscale_devices*

COPY src ./src
RUN cargo build --release

# ── Stage 2: runtime ──────────────────────────────────────────────────────────
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /build/target/release/tailscale-devices /usr/local/bin/tailscale-devices

EXPOSE 3453

# Run as the built-in nonroot user (uid 65532)
USER nonroot

ENTRYPOINT ["/usr/local/bin/tailscale-devices"]
