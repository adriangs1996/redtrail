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
    curl \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Install Node.js (required for Claude Code)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code globally
RUN npm install -g @anthropic-ai/claude-code

COPY --from=builder /usr/local/bin/redtrail /usr/local/bin/redtrail

# Copy test scripts
COPY eval/tests/ /tests/
RUN chmod +x /tests/*.sh

# Create a non-root user for realistic shell behavior
# UID/GID configurable via build args to match host (for volume mounts)
ARG USER_UID=1000
ARG USER_GID=1000
RUN groupadd -g ${USER_GID} testuser 2>/dev/null || true \
    && useradd -m -s /bin/zsh -u ${USER_UID} -g ${USER_GID} testuser
USER testuser
WORKDIR /home/testuser

# Skip Claude Code interactive onboarding (auth via CLAUDE_CODE_OAUTH_TOKEN env var)
RUN echo '{"hasCompletedOnboarding":true,"theme":"dark"}' > ~/.claude.json

RUN echo 'eval "$(redtrail init zsh)"' >> ~/.zshrc

# No ENTRYPOINT — test scripts are passed as the command to docker run
