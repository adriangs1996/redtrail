#!/bin/sh
ssh-keygen -A
/usr/sbin/sshd
/usr/sbin/vsftpd /etc/vsftpd.conf &
exec python /app/app.py
