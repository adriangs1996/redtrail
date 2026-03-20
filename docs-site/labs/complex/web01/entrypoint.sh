#!/bin/sh
python /app/bot.py &
exec python /app/app.py
