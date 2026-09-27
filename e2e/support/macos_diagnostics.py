"""Print bounded native crash diagnostics for CI fixture processes."""

import json
from pathlib import Path
import subprocess
import time

# Diagnose loader startup separately from the application. Each extra rule is
# applied only to a short-lived /bin/echo probe, never to Solmu or Boxer.
base = (
    '(version 1)(deny default)(allow process*)(allow signal)'
    '(allow sysctl-read)(allow mach-lookup)(allow network*)'
    '(allow file-read-metadata)'
    '(allow file-read* file-map-executable '
    '(subpath "/System")(subpath "/usr")(subpath "/bin")(subpath "/sbin"))'
    '(allow file-write* (literal "/dev/null")(literal "/dev/stdout")(literal "/dev/stderr"))'
)
for name, rule in (
    ("baseline", ""),
    ("root directory", '(allow file-read* (literal "/"))'),
    ("etc directories", '(allow file-read* (literal "/etc")(literal "/private/etc"))'),
    ("etc contents", '(allow file-read* (subpath "/private/etc"))'),
    ("preboot", '(allow file-read* file-map-executable (subpath "/private/preboot"))'),
    ("executable mapping", '(allow file-map-executable)'),
    ("system sockets", '(allow system-socket)'),
    ("self task name", '(allow mach-task-name (target self))'),
    ("syscalls", '(allow syscall*)'),
    ("all reads control", '(allow file-read*)'),
):
    try:
        result = subprocess.run(
            ["/usr/bin/sandbox-exec", "-p", base + rule, "--", "/bin/echo", "loader started"],
            capture_output=True, text=True, timeout=5,
        )
        print(f"Loader probe {name}: {result.returncode} {result.stdout.strip()} {result.stderr.strip()[:500]}")
    except (OSError, subprocess.TimeoutExpired) as error:
        print(f"Loader probe {name}: {error}")

directory = Path.home() / "Library/Logs/DiagnosticReports"
for attempt in range(5):
    reports = [
        path
        for name in ("sandbox-probe", "solmu-backend", "boxer")
        for path in directory.glob(f"{name}*.ips")
        if time.time() - path.stat().st_mtime < 600
    ]
    if reports:
        break
    time.sleep(1)

for path in sorted(reports, key=lambda path: path.stat().st_mtime)[-6:]:
    print(f"Crash report: {path.name}")
    try:
        source = path.read_text()
        decoder = json.JSONDecoder()
        _, offset = decoder.raw_decode(source)
        report = decoder.raw_decode(source[offset:].lstrip())[0]
        for key in ("exception", "termination", "asi"):
            print(f"{key}: {json.dumps(report.get(key))}")
        for thread in report.get("threads", []):
            if thread.get("triggered"):
                print(f"frames: {json.dumps(thread.get('frames', [])[:12])}")
    except (OSError, ValueError) as error:
        print(f"Could not read report: {error}")
