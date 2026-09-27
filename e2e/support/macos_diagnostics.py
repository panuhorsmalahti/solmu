"""Print bounded native crash diagnostics for CI fixture processes."""

import json
from pathlib import Path
import time

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
