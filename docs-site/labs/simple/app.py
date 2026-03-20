import os
import subprocess
from flask import Flask, request, render_template_string, redirect, session, url_for

app = Flask(__name__)
app.secret_key = os.urandom(24)

USERS = {"admin": "admin"}

LOGIN_PAGE = """
<!DOCTYPE html>
<html>
<head>
  <title>NetPanel - Login</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: monospace; background: #1e1e2e; color: #cdd6f4; display: flex; align-items: center; justify-content: center; height: 100vh; }
    .login-box { background: #313244; padding: 2rem; border-radius: 8px; width: 320px; }
    h2 { margin-bottom: 1rem; color: #f38ba8; }
    label { display: block; margin-top: 0.8rem; font-size: 0.9rem; color: #a6adc8; }
    input { width: 100%; padding: 0.5rem; margin-top: 0.3rem; background: #45475a; border: 1px solid #585b70; color: #cdd6f4; border-radius: 4px; font-family: monospace; }
    button { width: 100%; margin-top: 1.2rem; padding: 0.6rem; background: #f38ba8; border: none; color: #1e1e2e; font-weight: bold; font-family: monospace; border-radius: 4px; cursor: pointer; }
    button:hover { background: #eba0ac; }
    .error { color: #f38ba8; margin-top: 0.8rem; font-size: 0.85rem; }
  </style>
</head>
<body>
  <div class="login-box">
    <h2>NetPanel</h2>
    <form method="POST">
      <label>Username</label>
      <input name="username" type="text" autocomplete="off" />
      <label>Password</label>
      <input name="password" type="password" />
      <button type="submit">Login</button>
      {% if error %}<p class="error">{{ error }}</p>{% endif %}
    </form>
  </div>
</body>
</html>
"""

ADMIN_PAGE = """
<!DOCTYPE html>
<html>
<head>
  <title>NetPanel - Admin</title>
  <style>
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body { font-family: monospace; background: #1e1e2e; color: #cdd6f4; padding: 2rem; }
    nav { display: flex; justify-content: space-between; align-items: center; margin-bottom: 2rem; border-bottom: 1px solid #45475a; padding-bottom: 1rem; }
    nav h1 { color: #f38ba8; font-size: 1.2rem; }
    nav a { color: #a6adc8; text-decoration: none; font-size: 0.9rem; }
    nav a:hover { color: #f38ba8; }
    .panel { background: #313244; padding: 1.5rem; border-radius: 8px; max-width: 640px; }
    h3 { margin-bottom: 1rem; color: #a6e3a1; }
    label { display: block; margin-bottom: 0.3rem; color: #a6adc8; font-size: 0.9rem; }
    input[type=text] { width: 100%; padding: 0.5rem; background: #45475a; border: 1px solid #585b70; color: #cdd6f4; border-radius: 4px; font-family: monospace; }
    button { margin-top: 0.8rem; padding: 0.5rem 1.2rem; background: #a6e3a1; border: none; color: #1e1e2e; font-weight: bold; font-family: monospace; border-radius: 4px; cursor: pointer; }
    button:hover { background: #94e2d5; }
    .output { margin-top: 1.2rem; background: #11111b; padding: 1rem; border-radius: 4px; white-space: pre-wrap; font-size: 0.85rem; color: #a6adc8; max-height: 300px; overflow-y: auto; }
  </style>
</head>
<body>
  <nav>
    <h1>NetPanel Admin</h1>
    <a href="/logout">Logout</a>
  </nav>
  <div class="panel">
    <h3>Network Diagnostics</h3>
    <form method="POST">
      <label>Host to ping</label>
      <input name="host" type="text" placeholder="e.g. 192.168.1.1" autocomplete="off" />
      <button type="submit">Run Diagnostic</button>
    </form>
    {% if output %}
    <div class="output">{{ output }}</div>
    {% endif %}
  </div>
</body>
</html>
"""


@app.route("/", methods=["GET", "POST"])
def login():
    if session.get("user"):
        return redirect(url_for("admin"))
    error = None
    if request.method == "POST":
        username = request.form.get("username", "")
        password = request.form.get("password", "")
        if USERS.get(username) == password:
            session["user"] = username
            return redirect(url_for("admin"))
        error = "Invalid credentials"
    return render_template_string(LOGIN_PAGE, error=error)


@app.route("/admin", methods=["GET", "POST"])
def admin():
    if not session.get("user"):
        return redirect(url_for("login"))
    output = None
    if request.method == "POST":
        host = request.form.get("host", "")
        # VULNERABLE: unsanitized input passed to shell
        result = subprocess.run(
            f"ping -c 1 {host}",
            shell=True,
            capture_output=True,
            text=True,
            timeout=10,
        )
        output = result.stdout + result.stderr
    return render_template_string(ADMIN_PAGE, output=output)


@app.route("/logout")
def logout():
    session.clear()
    return redirect(url_for("login"))


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=80)
