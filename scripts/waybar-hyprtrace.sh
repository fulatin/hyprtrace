#!/bin/bash
# Waybar custom module for HyprTrace.
#
# Usage in ~/.config/waybar/config.jsonc:
#   "custom/hyprtrace": {
#     "exec": "~/.local/bin/waybar-hyprtrace.sh",
#     "interval": 60,
#     "return-type": "json",
#     "format": "{}"
#   }
#
# Requires: curl, python3.

API="${HYPRTRACE_API:-http://127.0.0.1:9420/api/status}"

data=$(curl -s --max-time 3 "$API" 2>/dev/null)

if [ -z "$data" ]; then
    printf '{"text":"⚙ —","tooltip":"HyprTrace unreachable"}\n'
    exit 0
fi

# Parse the API response and emit valid JSON (json.dumps escapes newlines/quotes).
printf '%s' "$data" | python3 -c '
import sys, json

try:
    d = json.load(sys.stdin)
    if d is None:
        raise ValueError("null response")
    app  = d.get("current_app") or "?"
    mins = int(float(d.get("current_session_min") or 0))
    pct  = int(float(d.get("today_pct_goal") or 0))
    score = int(float(d.get("efficiency_score") or 0))
except Exception:
    print(json.dumps({"text": "⚙ —", "tooltip": "HyprTrace: bad response"},
                     ensure_ascii=False))
    sys.exit(0)

if mins >= 60:
    dur = "%dh %dm" % (mins // 60, mins % 60)
else:
    dur = "%dm" % mins

text = "🧭 %s %s · %d%%" % (app, dur, pct)
tooltip = "Current: %s (%s)\nToday: %d%% of goal · Efficiency %d/100" % (app, dur, pct, score)

print(json.dumps({"text": text, "tooltip": tooltip}, ensure_ascii=False))
'
