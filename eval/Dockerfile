FROM rust:1.85-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src/ src/

RUN cargo build --release && cp target/release/redtrail /usr/local/bin/redtrail

# --- Runtime ---
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    zsh \
    bash \
    git \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/redtrail /usr/local/bin/redtrail

# Copy test scripts
COPY eval/tests/ /tests/
RUN chmod +x /tests/*.sh

# Create a non-root user for realistic shell behavior
RUN useradd -m -s /bin/zsh testuser
USER testuser
WORKDIR /home/testuser

RUN echo 'eval "$(redtrail init zsh)"' >> ~/.zshrc

# No ENTRYPOINT — test scripts are passed as the command to docker run
