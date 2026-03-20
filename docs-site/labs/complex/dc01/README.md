# dc01 — Domain Controller (Complex Lab)

Final target in the complex lab attack chain. Simulates a domain controller
where credentials discovered in db01's user dump grant admin-level access.

## Attack Chain

1. Discover `dc01admin:Domain@dmin2024!` in db01's `users` table (plaintext password, `domain_admin` role)
2. SSH into dc01 with those credentials (`ssh dc01admin@dc01`)
3. Use sudo to read the flag: `sudo cat /root/flag.txt`

## Services

| Service | Port (container) | Port (host) |
|---------|------------------|-------------|
| SSH     | 22               | 2223        |

## Credentials

| Username   | Password           | Source                     |
|------------|--------------------|----------------------------|
| dc01admin  | Domain@dmin2024!   | db01 `users` table dump    |

## Flag

Located at `/root/flag.txt`, readable only by root. The `dc01admin` user
has sudo privileges (simulating domain admin access).
