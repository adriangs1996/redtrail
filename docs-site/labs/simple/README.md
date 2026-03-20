# Simple Lab

Single-container vulnerable target with HTTP, FTP, and SSH services for practicing Redtrail commands.

## Services

| Service | Port | Details |
|---------|------|---------|
| HTTP | 80 | Flask web app — login `admin/admin`, command injection in admin panel |
| FTP | 21 | Anonymous login enabled, decoy files in `/pub` |
| SSH | 22 | Accepts password auth — `labuser/labuser`, `root/toor` |

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
- ~150 MB disk (Python slim image + packages)
- Ports 80, 21, 22, 40000-40100 available on host

## Attack Surface

- **HTTP**: Command injection via "Host to ping" field (`; cat /root/flag.txt`)
- **FTP**: Anonymous access exposes `backup.sql.gz` (junk) and `credentials.txt` (fake/expired creds)
- **SSH**: Weak credentials for lateral movement practice
