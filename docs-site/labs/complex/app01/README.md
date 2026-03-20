# Complex Lab — app01

Internal DevOps Dashboard with SSH access and command injection vulnerability.

## Services

| Service | Port | Details |
|---------|------|---------|
| SSH | 22 (mapped 2222) | OpenSSH — credentials reused from web01 (`devops:TechPulse2024!`) |
| HTTP | 8080 (mapped 8081) | Flask DevOps Dashboard — log search with command injection |

## Setup

```sh
docker compose up --build -d
```

## Teardown

```sh
docker compose down
```

## Resource Requirements

- Docker Engine 20.10+
- ~180 MB disk (Python slim + Flask + OpenSSH)
- Ports 2222 and 8081 available on host

## Attack Surface

- **SSH**: Credentials `devops:TechPulse2024!` (same as found on web01 after RCE)
- **Log Search**: Command injection via grep — unsanitized query passed to `shell=True`
- **Export**: Secondary command injection via awk service parameter (select element, but interceptable)
- **Config file**: `/etc/app/config.ini` contains plaintext database credentials for db01

## Attack Chain

1. Pivot from web01 using reused credentials → SSH into app01 as `devops`
2. Discover web dashboard on port 8080 (or access via browser after SSH tunnel)
3. Login with same credentials (`devops:TechPulse2024!`)
4. Log search field → command injection: `' ; cat /etc/app/config.ini ; '`
5. Retrieve db01 credentials from config: `tp_app:Pr0d-DB#Acc3ss!`
6. Pivot to db01 using discovered MySQL credentials
