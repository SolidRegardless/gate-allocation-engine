# ── Build stage ──────────────────────────────────────────────────────
FROM rust:1.85-slim AS builder

RUN apt-get update && apt-get install -y protobuf-compiler && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY build.rs ./
COPY proto/ proto/
COPY src/ src/

RUN cargo build --release

# ── Runtime stage ────────────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/gate_allocation_engine /usr/local/bin/

EXPOSE 50051

ENTRYPOINT ["gate_allocation_engine"]
CMD ["serve"]
