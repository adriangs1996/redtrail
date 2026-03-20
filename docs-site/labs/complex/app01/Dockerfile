FROM python:3.12-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends openssh-server && \
    rm -rf /var/lib/apt/lists/*

RUN pip install --no-cache-dir flask

RUN mkdir -p /run/sshd && \
    useradd -m -s /bin/bash devops && \
    echo "devops:TechPulse2024!" | chpasswd && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config

WORKDIR /app
COPY app.py .
COPY config.ini /etc/app/config.ini
COPY flag.txt /home/devops/flag.txt
RUN chmod 444 /home/devops/flag.txt && \
    chmod 644 /etc/app/config.ini

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22 8080

CMD ["/entrypoint.sh"]
