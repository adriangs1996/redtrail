FROM rust:1.93-slim-bookworm AS builder

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
  sqlite3 \
  && rm -rf /var/lib/apt/lists/*

# Install Node.js (required for Claude Code)
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
  && apt-get install -y nodejs \
  && rm -rf /var/lib/apt/lists/*

# Install Claude Code globally
RUN npm install -g @anthropic-ai/claude-code

# Install Starship prompt (only affects interactive shells, not test runs)
RUN curl -fsSL https://starship.rs/install.sh | sh -s -- --yes

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

COPY --chown=testuser:testuser eval/motd.sh /home/testuser/.motd.sh
COPY --chown=testuser:testuser eval/starship.toml /home/testuser/.config/starship.toml

RUN echo 'eval "$(redtrail init zsh)"' >> ~/.zshrc \
  && echo 'eval "$(starship init zsh)"' >> ~/.zshrc \
  && echo 'sh ~/.motd.sh' >> ~/.zshrc

# No ENTRYPOINT — test scripts are passed as the command to docker run
