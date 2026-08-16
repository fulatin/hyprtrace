#!/bin/bash
# Repair negative durations in HyprTrace's database.
#
# Background: when the wall clock jumps backwards (e.g. dual-booting Windows
# and Linux with RTC skew), `ended_at - started_at` becomes negative. The
# daemon now clamps new durations to zero and uses monotonic time; this script
# repairs rows that were recorded before the fix and rebuilds the summary
# tables so no negative totals remain.
#
# Usage: scripts/repair-negative-durations.sh [path-to-hyprtrace.db]

set -euo pipefail

DB="${1:-$HOME/.local/share/hyprtrace/hyprtrace.db}"

if [[ ! -f "$DB" ]]; then
    echo "Database not found: $DB" >&2
    exit 1
fi

command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 1; }

python3 - "$DB" <<'PY'
import sqlite3
import sys
import datetime
import os

db_path = sys.argv[1]
db = sqlite3.connect(db_path)
db.execute("PRAGMA busy_timeout=5000")

# 1) Consistent backup (honours WAL) next to the original database.
backup_path = f"{db_path}.bak-{datetime.datetime.now():%Y%m%d-%H%M%S}"
dst = sqlite3.connect(backup_path)
db.backup(dst)
dst.close()
print(f"Backup written to: {backup_path}")

def scalar(sql, args=()):
    return db.execute(sql, args).fetchone()[0]

neg_sessions = scalar("SELECT COUNT(*) FROM sessions WHERE duration_ms < 0")
neg_events = scalar("SELECT COUNT(*) FROM activity_events WHERE duration_ms < 0")
neg_daily = scalar("SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0 OR focused_ms < 0")
neg_hourly = scalar("SELECT COUNT(*) FROM hourly_summary WHERE total_ms < 0 OR focused_ms < 0")
print(f"Before: sessions={neg_sessions} activity_events={neg_events} daily_summary={neg_daily} hourly_summary={neg_hourly}")

# 2) Clamp negative durations to zero.
db.execute(
    "UPDATE sessions SET duration_ms = 0, focused_ms = MAX(COALESCE(focused_ms, 0), 0) "
    "WHERE duration_ms < 0"
)
db.execute("UPDATE activity_events SET duration_ms = 0 WHERE duration_ms < 0")

# 3) Rebuild daily_summary from the repaired sessions.
db.execute("DELETE FROM daily_summary")
db.execute(
    """
    INSERT INTO daily_summary
        (date, class, total_ms, session_count, focused_ms, focused_session_count)
    SELECT date(started_at),
           class,
           SUM(duration_ms),
           COUNT(*),
           SUM(COALESCE(focused_ms, 0)),
           SUM(CASE WHEN COALESCE(focused_ms, 0) > 0 THEN 1 ELSE 0 END)
    FROM sessions
    WHERE ended_at IS NOT NULL AND duration_ms >= 0
    GROUP BY date(started_at), class
    """
)

# 4) Rebuild hourly_summary bucketed by LOCAL start hour, matching the daemon.
from collections import defaultdict
rows = db.execute(
    "SELECT started_at, class, duration_ms, COALESCE(focused_ms, 0) "
    "FROM sessions WHERE ended_at IS NOT NULL AND duration_ms >= 0"
).fetchall()
agg = defaultdict(lambda: [0, 0, 0])
for started_at, cls, duration_ms, focused_ms in rows:
    try:
        dt = datetime.datetime.fromisoformat(started_at)
    except ValueError:
        continue
    local = dt.astimezone()  # system local timezone, same as chrono::Local
    key = (local.date().isoformat(), local.hour, cls)
    entry = agg[key]
    entry[0] += duration_ms
    entry[1] += 1
    entry[2] += focused_ms

db.execute("DELETE FROM hourly_summary")
for (date, hour, cls), (total_ms, session_count, focused_ms) in sorted(agg.items()):
    db.execute(
        "INSERT INTO hourly_summary (date, hour, class, total_ms, session_count, focused_ms) "
        "VALUES (?, ?, ?, ?, ?, ?)",
        (date, hour, cls, total_ms, session_count, focused_ms),
    )

db.commit()

neg_sessions = scalar("SELECT COUNT(*) FROM sessions WHERE duration_ms < 0")
neg_events = scalar("SELECT COUNT(*) FROM activity_events WHERE duration_ms < 0")
neg_daily = scalar("SELECT COUNT(*) FROM daily_summary WHERE total_ms < 0 OR focused_ms < 0")
neg_hourly = scalar("SELECT COUNT(*) FROM hourly_summary WHERE total_ms < 0 OR focused_ms < 0")
print(f"After:  sessions={neg_sessions} activity_events={neg_events} daily_summary={neg_daily} hourly_summary={neg_hourly}")
print("Done.")
PY
