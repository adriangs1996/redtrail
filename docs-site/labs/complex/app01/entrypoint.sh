#!/bin/sh
/usr/sbin/sshd
exec python /app/app.py
