# Complex Lab — TechPulse Corporate Network

Multi-container pentesting lab simulating a corporate network with DMZ segmentation.

## Network Topology

```
                    ┌─────────────────────────────────────────────┐
                    │              internal network                │
  Host Machine      │   (bridge, internal: true — no host access) │
  ─────────────     │                                             │
       │            │   ┌────────┐  SSH   ┌────────┐             │
       │            │   │ app01  │───────→│  dc01  │             │
       │ :8080      │   │ :8080  │        │  :22   │             │
       │            │   │  :22   │        └────────┘             │
       ▼            │   └────┬───┘             ▲                 │
  ┌─────────┐       │        │ MySQL           │ SSH             │
  │  web01  │───────┼────────┤                 │ (creds from db) │
  │  :80    │  DMZ  │        ▼                 │                 │
  └─────────┘ pivot │   ┌────────┐             │                 │
       ▲            │   │  db01  │─────────────┘                 │
       │            │   │ :3306  │  (stores dc01 creds)          │
  ─────┴─────       │   └────────┘                               │
  external net      └─────────────────────────────────────────────┘
  (bridge)
```

## Attack Chain

1. **web01** (`:8080` from host) — Dir enum → stored XSS in guestbook → steal admin cookie via bot → admin panel → Jinja2 SSTI → RCE → find SSH creds (`devops:TechPulse2024!`)
2. **app01** (pivot from web01 via SSH) — Command injection in log search → read `/etc/app/config.ini` → get DB creds (`tp_app:Pr0d-DB#Acc3ss!`)
3. **db01** (pivot from app01 via MySQL client) — Query `users` table → find `dc01admin:Domain@dmin2024!` + `LOAD_FILE('/var/lib/mysql-files/flag.txt')`
4. **dc01** (pivot from app01 via SSH) — Login with domain admin creds → `sudo cat /root/flag.txt`

## Networks

| Network    | Type     | Host Access | Containers                  |
|------------|----------|-------------|-----------------------------|
| `external` | bridge   | Yes         | web01                       |
| `internal` | bridge   | No          | web01, app01, db01, dc01    |

Only `web01` is reachable from the host (port 8080). All other containers are isolated on the internal network. `web01` bridges both networks as the DMZ pivot point.

## Setup

```bash
docker compose up --build -d
```

Wait for health checks to pass (~30-40s for db01 MySQL init):

```bash
docker compose ps
```

All services should show `healthy` status.

## Teardown

```bash
docker compose down -v
```

The `-v` flag removes MySQL volumes to ensure clean state on next run.

## Resource Requirements

| Container | Base Image           | Approx Size | RAM   |
|-----------|---------------------|-------------|-------|
| web01     | python:3.12-slim    | ~150 MB     | ~50 MB  |
| app01     | python:3.12-slim    | ~180 MB     | ~50 MB  |
| db01      | mysql:8.0           | ~550 MB     | ~400 MB |
| dc01      | debian:bookworm-slim| ~100 MB     | ~20 MB  |
| **Total** |                     | **~980 MB** | **~520 MB** |

## Verifying Isolation

From the host, only web01 should be reachable:

```bash
curl http://localhost:8080          # works
docker exec complex-web01 ping -c1 app01   # works (internal net)
docker exec complex-web01 ping -c1 db01    # works (internal net)
```

app01, db01, dc01 have no published ports — not accessible from host.

## Flags

Each container has a flag file. Retrieve all 4 to complete the lab.

| Container | Flag Location                        | Access Method         |
|-----------|--------------------------------------|-----------------------|
| web01     | `/root/flag.txt`                     | RCE via SSTI          |
| app01     | `/root/flag.txt`                     | RCE via cmd injection |
| db01      | `/var/lib/mysql-files/flag.txt`      | `LOAD_FILE()` via SQL |
| dc01      | `/root/flag.txt`                     | `sudo cat` via SSH    |
