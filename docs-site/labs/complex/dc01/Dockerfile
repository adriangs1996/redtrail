FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends openssh-server sudo && \
    rm -rf /var/lib/apt/lists/*

RUN mkdir -p /run/sshd && \
    useradd -m -s /bin/bash dc01admin && \
    echo "dc01admin:Domain@dmin2024!" | chpasswd && \
    usermod -aG sudo dc01admin && \
    echo "dc01admin ALL=(ALL) NOPASSWD: ALL" > /etc/sudoers.d/dc01admin && \
    sed -i 's/#PermitRootLogin.*/PermitRootLogin no/' /etc/ssh/sshd_config && \
    sed -i 's/#PasswordAuthentication.*/PasswordAuthentication yes/' /etc/ssh/sshd_config

COPY flag.txt /root/flag.txt
RUN chmod 400 /root/flag.txt

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 22

CMD ["/entrypoint.sh"]
