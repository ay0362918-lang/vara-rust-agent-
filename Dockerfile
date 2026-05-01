FROM rust:1.85-slim

WORKDIR /app

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml .
COPY src/ src/

RUN cargo build --release

CMD ["./target/release/hy4-spammer"]
