FROM rust:1.85 AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY contracts/ ./contracts/
COPY circuits/ ./circuits/
COPY scripts/ ./scripts/
COPY stellar-zk-cards/ ./stellar-zk-cards/
COPY services/ ./services/
COPY vendor/ ./vendor/
RUN ./scripts/compile-circuits.sh
RUN cargo build --release -p coordinator

# Build co-noir from source
FROM rust:1.85 AS conoir-builder
RUN cargo install --git https://github.com/TaceoLabs/co-snarks --branch main co-noir

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/coordinator /usr/local/bin/coordinator
COPY --from=conoir-builder /usr/local/cargo/bin/co-noir /usr/local/bin/co-noir
COPY --from=builder /app/circuits /app/circuits

CMD ["coordinator"]
