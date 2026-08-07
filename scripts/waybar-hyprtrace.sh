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
# Requires: curl, python3 (or jq).

API="${HYPRTRACE_API:-http://127.0.0.1:9420/api/status}"

data=$(curl -s --max-time 3 "$API" 2>/dev/null)
if [ -z "$data" ] || [ "$data" = "null" ]; then
    printf '{"text":"⚙ —","tooltip":"HyprTrace unreachable"}\n'
    exit 0
fi

# Extract values. Prefer jq if available, else python3.
if command -v jq >/dev/null 2>&1; then
    app=$(echo "$data" | jq -r '.current_app')
    mins=$(echo "$data" | jq -r '.current_session_min')
    pct=$(echo "$data" | jq -r '.today_pct_goal | floor')
    score=$(echo "$data" | jq -r '.efficiency_score // 0')
else
    app=$(echo "$data" | python3 -c "import sys,json;print(json.load(sys.stdin)['current_app'])")
    mins=$(echo "$data" | python3 -c "import sys,json;print(json.load(sys.stdin)['current_session_min'])")
    pct=$(echo "$data" | python3 -c "import sys,json;print(int(json.load(sys.stdin)['today_pct_goal']))")
    score=$(echo "$data" | python3 -c "import sys,json;print(json.load(sys.stdin).get('efficiency_score') or 0)")
fi

# Human-readable session duration.
if [ "$mins" -ge 60 ]; then
    dur="$((mins / 60))h $((mins % 60))m"
else
    dur="${mins}m"
fi

tooltip="Current: $app ($dur)
Today: $pct% of goal · Efficiency $score/100"

# Waybar supports click actions via signal/actions; keep it simple.
printf '{"text":"%s","tooltip":"%s"}\n' "🧭 ${app} ${dur} · ${pct}%" "$tooltip"
