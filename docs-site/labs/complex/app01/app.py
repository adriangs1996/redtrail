import os
import subprocess
from flask import Flask, request, render_template_string, redirect, url_for, session

app = Flask(__name__)
app.secret_key = os.environ.get("SECRET_KEY", "app01-internal-key")

USERS = {
    "devops": "TechPulse2024!",
}

STYLE = """
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: 'Segoe UI', monospace; background: #1e1e2e; color: #cdd6f4; }
a { color: #89b4fa; text-decoration: none; }
a:hover { text-decoration: underline; }
nav { background: #181825; padding: 1rem 2rem; display: flex; justify-content: space-between; align-items: center; border-bottom: 1px solid #313244; }
nav .brand { color: #f38ba8; font-weight: bold; font-size: 1.1rem; }
nav .links a { margin-left: 1.5rem; color: #a6adc8; font-size: 0.9rem; }
nav .links a:hover { color: #cdd6f4; }
.container { max-width: 900px; margin: 2rem auto; padding: 0 1rem; }
h1, h2, h3 { color: #cdd6f4; margin-bottom: 1rem; }
.card { background: #313244; padding: 1.5rem; border-radius: 8px; margin-bottom: 1.5rem; }
input[type=text], input[type=password], select {
    width: 100%; padding: 0.6rem; background: #45475a; border: 1px solid #585b70;
    color: #cdd6f4; border-radius: 4px; font-family: monospace; margin-top: 0.3rem;
}
label { display: block; margin-top: 0.8rem; font-size: 0.9rem; color: #a6adc8; }
button, input[type=submit] {
    margin-top: 1rem; padding: 0.5rem 1.2rem; background: #a6e3a1; border: none;
    color: #1e1e2e; font-weight: bold; font-family: monospace; border-radius: 4px; cursor: pointer;
}
button:hover, input[type=submit]:hover { background: #94e2d5; }
.output { background: #11111b; padding: 1rem; border-radius: 4px; white-space: pre-wrap; font-size: 0.85rem; color: #a6adc8; max-height: 500px; overflow-y: auto; margin-top: 1rem; }
.error { color: #f38ba8; margin-top: 0.5rem; font-size: 0.85rem; }
.success { color: #a6e3a1; margin-top: 0.5rem; font-size: 0.85rem; }
footer { text-align: center; padding: 2rem; color: #585b70; font-size: 0.8rem; }
table { width: 100%; border-collapse: collapse; margin-top: 1rem; }
th, td { text-align: left; padding: 0.5rem 0.8rem; border-bottom: 1px solid #45475a; font-size: 0.85rem; }
th { color: #a6adc8; font-weight: normal; text-transform: uppercase; font-size: 0.75rem; }
.badge { display: inline-block; padding: 0.15rem 0.5rem; border-radius: 3px; font-size: 0.75rem; }
.badge-ok { background: #a6e3a1; color: #1e1e2e; }
.badge-warn { background: #f9e2af; color: #1e1e2e; }
.badge-err { background: #f38ba8; color: #1e1e2e; }
"""

LAYOUT_TOP = """<!DOCTYPE html>
<html><head><title>DevOps Dashboard - {{ title }}</title>
<style>""" + STYLE + """</style></head><body>
<nav>
  <span class="brand">⚙ DevOps Dashboard</span>
  <span class="links">
    {% if session.get('user') %}
      <a href="/">Status</a>
      <a href="/logs">Logs</a>
      <a href="/export">Export</a>
      <a href="/logout">Logout</a>
    {% else %}
      <a href="/login">Login</a>
    {% endif %}
  </span>
</nav>
<div class="container">"""

LAYOUT_BOTTOM = """</div>
<footer>DevOps Dashboard v2.1.4 — Internal Use Only</footer>
</body></html>"""

LOG_DATA = """2026-03-18 08:12:01 INFO  [web01] Health check passed
2026-03-18 08:12:03 INFO  [app01] Service started on port 8080
2026-03-18 08:12:05 INFO  [db01] Connection pool initialized (max=20)
2026-03-18 08:12:10 WARN  [web01] High memory usage: 78%
2026-03-18 08:12:15 INFO  [app01] Request processed: GET /api/status 200 12ms
2026-03-18 08:12:20 ERROR [db01] Slow query detected: SELECT * FROM sessions (2.3s)
2026-03-18 08:12:25 INFO  [web01] SSL certificate valid for 45 days
2026-03-18 08:12:30 WARN  [app01] Deprecated API call: /api/v1/users
2026-03-18 08:12:35 INFO  [db01] Backup completed successfully
2026-03-18 08:12:40 ERROR [web01] 502 Bad Gateway from upstream app01:8080
2026-03-18 08:12:45 INFO  [app01] Cache hit ratio: 94.2%
2026-03-18 08:12:50 INFO  [db01] Replication lag: 0.02s
2026-03-18 08:13:01 WARN  [web01] Rate limit triggered for 10.0.1.50
2026-03-18 08:13:05 INFO  [app01] Deployment v2.1.4 healthy
2026-03-18 08:13:10 ERROR [db01] Lock wait timeout on table: orders
2026-03-18 08:13:15 INFO  [web01] CDN cache purged for /static/*
2026-03-18 08:13:20 INFO  [app01] Background job completed: report_gen (3.1s)
2026-03-18 08:13:25 WARN  [db01] Disk usage at 82%
2026-03-18 08:13:30 INFO  [web01] New session created: user=admin
2026-03-18 08:13:35 INFO  [app01] API key rotated for service: analytics
"""


@app.route("/")
def index():
    if not session.get("user"):
        return redirect(url_for("login"))
    return render_template_string(LAYOUT_TOP + """
<h2>System Status</h2>
<div class="card">
    <table>
        <tr><th>Service</th><th>Status</th><th>Uptime</th><th>CPU</th><th>Memory</th></tr>
        <tr><td>web01</td><td><span class="badge badge-ok">UP</span></td><td>14d 6h</td><td>23%</td><td>78%</td></tr>
        <tr><td>app01</td><td><span class="badge badge-ok">UP</span></td><td>14d 6h</td><td>45%</td><td>62%</td></tr>
        <tr><td>db01</td><td><span class="badge badge-warn">WARN</span></td><td>14d 6h</td><td>67%</td><td>82%</td></tr>
        <tr><td>dc01</td><td><span class="badge badge-ok">UP</span></td><td>14d 6h</td><td>12%</td><td>34%</td></tr>
    </table>
</div>
<div class="card">
    <h3>Quick Actions</h3>
    <p style="margin-top:0.5rem; color: #a6adc8;">
        <a href="/logs">Search Logs</a> &middot;
        <a href="/export">Export Report</a>
    </p>
</div>
""" + LAYOUT_BOTTOM, title="Status", session=session)


@app.route("/login", methods=["GET", "POST"])
def login():
    error = ""
    if request.method == "POST":
        user = request.form.get("username", "")
        pw = request.form.get("password", "")
        if user in USERS and USERS[user] == pw:
            session["user"] = user
            return redirect(url_for("index"))
        error = "Invalid credentials"
    return render_template_string(LAYOUT_TOP + """
<h2>Login</h2>
<div class="card">
    <form method="POST">
        <label>Username</label>
        <input type="text" name="username" autocomplete="off">
        <label>Password</label>
        <input type="password" name="password">
        <input type="submit" value="Sign In">
    </form>
    {% if error %}<p class="error">{{ error }}</p>{% endif %}
</div>
""" + LAYOUT_BOTTOM, title="Login", error=error, session=session)


@app.route("/logout")
def logout():
    session.clear()
    return redirect(url_for("login"))


@app.route("/logs", methods=["GET", "POST"])
def logs():
    if not session.get("user"):
        return redirect(url_for("login"))

    output = ""
    query = ""
    if request.method == "POST":
        query = request.form.get("query", "")
        if query:
            # VULNERABLE: command injection via unsanitized grep argument
            cmd = f"grep -i '{query}' /var/log/app/service.log"
            try:
                result = subprocess.run(
                    cmd, shell=True, capture_output=True, text=True, timeout=5
                )
                output = result.stdout or result.stderr or "(no matches)"
            except subprocess.TimeoutExpired:
                output = "(search timed out)"

    return render_template_string(LAYOUT_TOP + """
<h2>Log Search</h2>
<div class="card">
    <form method="POST">
        <label>Search pattern</label>
        <input type="text" name="query" value="{{ query }}" placeholder="e.g. ERROR, web01, timeout">
        <input type="submit" value="Search">
    </form>
    {% if output %}
    <div class="output">{{ output }}</div>
    {% endif %}
</div>
<div class="card" style="font-size:0.85rem; color:#6c7086;">
    <p>Searches service logs using pattern matching. Supports basic text patterns.</p>
</div>
""" + LAYOUT_BOTTOM, title="Logs", output=output, query=query, session=session)


@app.route("/export", methods=["GET", "POST"])
def export():
    if not session.get("user"):
        return redirect(url_for("login"))

    output = ""
    fmt = ""
    if request.method == "POST":
        fmt = request.form.get("format", "csv")
        service = request.form.get("service", "all")
        # VULNERABLE: command injection via service name in awk command
        cmd = f"awk '/\\[{service}\\]/ {{print}}' /var/log/app/service.log"
        try:
            result = subprocess.run(
                cmd, shell=True, capture_output=True, text=True, timeout=5
            )
            lines = result.stdout.strip()
            if fmt == "csv":
                output = "timestamp,level,service,message\n"
                for line in (lines or "").split("\n"):
                    if line.strip():
                        output += line.replace(" ", ",", 3) + "\n"
            else:
                output = lines or "(no data)"
        except subprocess.TimeoutExpired:
            output = "(export timed out)"

    return render_template_string(LAYOUT_TOP + """
<h2>Export Report</h2>
<div class="card">
    <form method="POST">
        <label>Service</label>
        <select name="service">
            <option value="all">All Services</option>
            <option value="web01">web01</option>
            <option value="app01">app01</option>
            <option value="db01">db01</option>
            <option value="dc01">dc01</option>
        </select>
        <label>Format</label>
        <select name="format">
            <option value="csv">CSV</option>
            <option value="raw">Raw Text</option>
        </select>
        <input type="submit" value="Export">
    </form>
    {% if output %}
    <div class="output">{{ output }}</div>
    {% endif %}
</div>
""" + LAYOUT_BOTTOM, title="Export", output=output, fmt=fmt, session=session)


if __name__ == "__main__":
    os.makedirs("/var/log/app", exist_ok=True)
    with open("/var/log/app/service.log", "w") as f:
        f.write(LOG_DATA.strip() + "\n")
    app.run(host="0.0.0.0", port=8080)
