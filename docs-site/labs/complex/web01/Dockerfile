FROM python:3.12-slim

RUN pip install --no-cache-dir flask requests

WORKDIR /app
COPY app.py .
COPY bot.py .
COPY flag.txt /root/flag.txt
RUN chmod 400 /root/flag.txt

COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

EXPOSE 80

CMD ["/entrypoint.sh"]
