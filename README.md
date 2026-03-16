# Redtrail - Gray Box Agentic Security Scanner

An LLM-powered security scanner that autonomously discovers and tests web application vulnerabilities.

## Quick Start

```bash
cargo build
cargo run -- scan --target http://TARGET_URL --llm claude --output report.html
```

## CLI Options

```
cargo run -- scan [OPTIONS]
  --target <URL>         Target URL to scan
  --llm <PROVIDER>       LLM provider: claude or ollama (default: claude)
  --max-turns <N>        Maximum agent turns (default: 20)
  --output <PATH>        Output path for HTML report
  --auth-token <TOKEN>   JWT authentication token
  --auth-login <U:P>     Auto-login credentials (user:pass)
  --verbose              Show full agent reasoning output
```

## Testing against labs

### Prerequisites

- Docker and Docker Compose installed
- The hacker-lab repository cloned at `/Users/adriangonzalez/Projects/hacker-lab`
- Claude CLI (`claude`) available in PATH

### Running the SQL injection lab

1. Start the lab environment (module 4 - Web Injection):

```bash
cd /Users/adriangonzalez/Projects/hacker-lab
./lab.sh start 4
```

2. Wait for the MySQL database to become healthy:

```bash
curl http://172.20.4.20/health
# Should return "OK" when ready
```

3. Run redtrail against the sqli-easy container:

```bash
cd redtrail
cargo run -- scan --target http://172.20.4.20 --llm claude --output test-sqli-report.html
```

4. Expected results:
   - The agent discovers the `/search` endpoint with the `q` query parameter
   - SQL injection is detected and confirmed (error-based and UNION-based)
   - Data is extracted via UNION-based injection (table names, user credentials)
   - The report includes at least one Critical finding for SQL injection
   - The security score is below 50
   - Fix suggestions mention parameterized queries

5. Stop the lab when done:

```bash
cd /Users/adriangonzalez/Projects/hacker-lab
./lab.sh stop 4
```

### Running integration tests

Integration tests require the lab to be running. They are marked `#[ignore]` so they don't run during normal `cargo test`.

```bash
# Start the lab first
cd /Users/adriangonzalez/Projects/hacker-lab && ./lab.sh start 4

# Run integration tests (from the redtrail directory)
cd redtrail
cargo test --test integration_sqli -- --ignored --nocapture

# Stop the lab after testing
cd /Users/adriangonzalez/Projects/hacker-lab && ./lab.sh stop 4
```

### Available lab targets

| Container    | IP            | Description                        |
|-------------|---------------|------------------------------------|
| sqli-easy   | 172.20.4.20   | UNION-based SQLi, no filtering     |
| sqli-blind  | 172.20.4.30   | Boolean + time-based blind SQLi    |
| sqli-hard   | 172.20.4.40   | WAF filtering with bypasses        |
| cmdi        | 172.20.4.50   | Command injection                  |

## Running unit tests

```bash
cargo test
```

## Linting

```bash
cargo clippy -- -D warnings
```
