import os
import html
import uuid
import hashlib
import time
from flask import (
    Flask, request, render_template_string, redirect,
    session, url_for, make_response, jsonify,
)

app = Flask(__name__)
app.secret_key = os.environ.get("SECRET_KEY", "s3cr3t-k3y-for-lab")

ADMIN_TOKEN = os.environ.get("ADMIN_TOKEN", "admintok_" + hashlib.md5(b"redtrail-complex-lab").hexdigest())

guestbook_entries = []

STYLE = """
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: 'Segoe UI', monospace; background: #1e1e2e; color: #cdd6f4; }
a { color: #89b4fa; text-decoration: none; }
a:hover { text-decoration: underline; }
nav { background: #181825; padding: 1rem 2rem; display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid #313244; }
nav .brand { color: #f38ba8; font-weight: bold; font-size: 1.1rem; }
nav .links a { margin-left: 1.5rem; color: #a6adc8; font-size: 0.9rem; }
nav .links a:hover { color: #cdd6f4; }
.container { max-width: 800px; margin: 2rem auto; padding: 0 1rem; }
h1, h2, h3 { color: #cdd6f4; margin-bottom: 1rem; }
.card { background: #313244; padding: 1.5rem; border-radius: 8px; margin-bottom: 1.5rem; }
input[type=text], input[type=password], textarea {
    width: 100%; padding: 0.6rem; background: #45475a; border: 1px solid #585b70;
    color: #cdd6f4; border-radius: 4px; font-family: monospace; margin-top: 0.3rem;
}
textarea { min-height: 80px; resize: vertical; }
label { display: block; margin-top: 0.8rem; font-size: 0.9rem; color: #a6adc8; }
button, input[type=submit] {
    margin-top: 1rem; padding: 0.5rem 1.2rem; background: #a6e3a1; border: none;
    color: #1e1e2e; font-weight: bold; font-family: monospace; border-radius: 4px; cursor: pointer;
}
button:hover, input[type=submit]:hover { background: #94e2d5; }
.entry { background: #45475a; padding: 1rem; border-radius: 6px; margin-bottom: 0.8rem; }
.entry .meta { font-size: 0.8rem; color: #6c7086; margin-bottom: 0.4rem; }
.error { color: #f38ba8; margin-top: 0.5rem; font-size: 0.85rem; }
.success { color: #a6e3a1; margin-top: 0.5rem; font-size: 0.85rem; }
.output { background: #11111b; padding: 1rem; border-radius: 4px; white-space: pre-wrap; font-size: 0.85rem; color: #a6adc8; max-height: 400px; overflow-y: auto; margin-top: 1rem; }
footer { text-align: center; padding: 2rem; color: #585b70; font-size: 0.8rem; }
"""

LAYOUT_TOP = """<!DOCTYPE html>
<html><head><title>TechPulse - {{ title }}</title>
<style>""" + STYLE + """</style></head><body>
<nav>
  <span class="brand">TechPulse</span>
  <div class="links">
    <a href="/">Home</a>
    <a href="/guestbook">Guestbook</a>
    <a href="/about">About</a>
  </div>
</nav>
<div class="container">
"""

LAYOUT_BOTTOM = """
</div>
<footer>TechPulse &copy; 2025 — Powered by TechPulse Engine v3.2.1</footer>
</body></html>
"""


@app.after_request
def add_headers(response):
    response.headers["X-Powered-By"] = "Apache/2.4.49"
    response.headers["Server"] = "Apache/2.4.49 (Ubuntu)"
    return response


@app.route("/")
def index():
    return render_template_string(LAYOUT_TOP + """
    <h1>Welcome to TechPulse</h1>
    <div class="card">
      <p>TechPulse is a cutting-edge technology blog and community platform. Share your thoughts in our
      <a href="/guestbook">guestbook</a> and connect with fellow tech enthusiasts.</p>
    </div>
    <div class="card">
      <h3>Latest Updates</h3>
      <p style="color:#6c7086;">Infrastructure migration completed. All systems nominal.</p>
    </div>
    """ + LAYOUT_BOTTOM, title="Home")


@app.route("/about")
def about():
    return render_template_string(LAYOUT_TOP + """
    <h1>About TechPulse</h1>
    <div class="card">
      <p>TechPulse was founded in 2023 as a community-driven tech platform.</p>
      <p style="margin-top:0.8rem; color:#6c7086;">Running TechPulse Engine v3.2.1 on Apache/2.4.49</p>
    </div>
    """ + LAYOUT_BOTTOM, title="About")


@app.route("/guestbook", methods=["GET", "POST"])
def guestbook():
    msg = ""
    if request.method == "POST":
        name = request.form.get("name", "").strip()
        message = request.form.get("message", "").strip()
        if name and message:
            guestbook_entries.append({
                "id": str(uuid.uuid4())[:8],
                "name": html.escape(name),
                "message": message,  # VULNERABLE: stored XSS — message not escaped
                "time": time.strftime("%Y-%m-%d %H:%M"),
            })
            msg = "Message posted!"
        else:
            msg = "Name and message are required."

    entries_html = ""
    for entry in reversed(guestbook_entries):
        entries_html += f"""
        <div class="entry">
          <div class="meta">{entry['name']} — {entry['time']}</div>
          <div>{entry['message']}</div>
        </div>
        """

    return render_template_string(LAYOUT_TOP + """
    <h1>Guestbook</h1>
    <div class="card">
      <h3>Leave a message</h3>
      <form method="POST">
        <label>Name</label>
        <input type="text" name="name" autocomplete="off" />
        <label>Message</label>
        <textarea name="message"></textarea>
        <input type="submit" value="Post" />
      </form>
      {% if msg %}<p class="success">{{ msg }}</p>{% endif %}
    </div>
    <h2 style="margin-top:1.5rem;">Messages</h2>
    """ + entries_html + LAYOUT_BOTTOM, title="Guestbook", msg=msg)


# --- Red herrings ---

@app.route("/robots.txt")
def robots():
    body = """User-agent: *
Disallow: /backup/
Disallow: /internal-docs/
Disallow: /admin-portal/
Disallow: /api/debug
"""
    resp = make_response(body, 200)
    resp.headers["Content-Type"] = "text/plain"
    return resp


@app.route("/api/debug")
def api_debug():
    return jsonify({
        "status": "ok",
        "debug": False,
        "version": "3.2.1",
        "server": "Apache/2.4.49",
        "uptime": "14d 3h 22m",
        "note": "Debug mode disabled in production. Contact sysadmin.",
    })


@app.route("/backup/")
@app.route("/backup/<path:p>")
def backup(p=""):
    return make_response("403 Forbidden", 403)


@app.route("/internal-docs/")
@app.route("/internal-docs/<path:p>")
def internal_docs(p=""):
    return make_response("403 Forbidden", 403)


# --- Hidden admin panel (discoverable via dir enum) ---

@app.route("/admin-portal", methods=["GET", "POST"])
def admin_portal():
    token = request.cookies.get("session_token")
    if token != ADMIN_TOKEN:
        return make_response(
            render_template_string(LAYOUT_TOP + """
            <h1>Admin Portal</h1>
            <div class="card">
              <p class="error">Access denied. Valid session required.</p>
            </div>
            """ + LAYOUT_BOTTOM, title="Admin"),
            403,
        )

    output = ""
    if request.method == "POST":
        template_code = request.form.get("template", "")
        try:
            # VULNERABLE: Jinja2 SSTI — user-controlled template rendered directly
            output = render_template_string(template_code)
        except Exception as e:
            output = f"Template error: {e}"

    return render_template_string(LAYOUT_TOP + """
    <h1>Admin Portal</h1>
    <div class="card">
      <h3>Template Preview</h3>
      <p style="color:#6c7086;">Preview email templates before sending to subscribers.</p>
      <form method="POST">
        <label>Template code (Jinja2)</label>
        <textarea name="template" rows="6" placeholder="Hello {{ '{{' }} name {{ '}}' }}, welcome!"></textarea>
        <input type="submit" value="Render Preview" />
      </form>
      {% if output %}
      <div class="output">{{ output }}</div>
      {% endif %}
    </div>
    """ + LAYOUT_BOTTOM, title="Admin", output=output)


if __name__ == "__main__":
    print(f"[*] Admin token: {ADMIN_TOKEN}")
    app.run(host="0.0.0.0", port=80)
