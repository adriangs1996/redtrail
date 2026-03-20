FROM python:3.12-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends openssh-server vsftpd && \
    rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir flask

RUN mkdir -p /run/sshd && \
    echo 'root:toor' | chpasswd && \
    sed -i 's/#PermitRootLogin prohibit-password/PermitRootLogin yes/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication yes/PasswordAuthentication yes/' /etc/ssh/sshd_config

RUN useradd -m labuser && echo 'labuser:labuser' | chpasswd

RUN printf "listen=YES\nlisten_ipv6=NO\nanonymous_enable=YES\nlocal_enable=NO\nwrite_enable=NO\nanon_root=/srv/ftp\npasv_enable=YES\npasv_min_port=40000\npasv_max_port=40100\nseccomp_sandbox=NO\n" > /etc/vsftpd.conf

RUN mkdir -p /srv/ftp/pub
COPY ftp-data/backup.sql.gz /srv/ftp/pub/backup.sql.gz
COPY ftp-data/credentials.txt /srv/ftp/pub/credentials.txt
RUN chmod -R 444 /srv/ftp/pub && chmod 555 /srv/ftp /srv/ftp/pub

WORKDIR /app
COPY app.py .
COPY flag.txt /root/flag.txt
RUN chmod 400 /root/flag.txt

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 80 21 22 40000-40100

CMD ["/entrypoint.sh"]
