# db01 — MySQL Database Server

## Overview
MySQL 8.0 database server for the TechPulse complex lab. Contains production user data including domain controller admin credentials in plaintext.

## Attack Chain
1. Connect using credentials from app01's `/etc/app/config.ini`: `tp_app:Pr0d-DB#Acc3ss!`
2. Enumerate tables: `SHOW TABLES;` → find `users` table
3. Dump users: `SELECT * FROM users;` → find `dc01admin` with plaintext password `Domain@dmin2024!`
4. The `tp_app` user has `FILE` privilege → `SELECT LOAD_FILE('/var/lib/mysql-files/flag.txt');` to read the flag
5. Use dc01admin credentials to pivot to dc01

## Credentials
| User | Password | Privileges |
|------|----------|------------|
| tp_app | Pr0d-DB#Acc3ss! | SELECT, INSERT, UPDATE, DELETE, FILE |
| root | r00t_TechPulse_S3cret | ALL |

## Ports
- 3306 (MySQL) → mapped to 3307 on host
