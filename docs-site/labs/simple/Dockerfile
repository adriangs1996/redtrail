FROM python:3.12-slim

RUN pip install --no-cache-dir flask

WORKDIR /app
COPY app.py .
COPY flag.txt /root/flag.txt

RUN chmod 400 /root/flag.txt

EXPOSE 80

CMD ["python", "app.py"]
