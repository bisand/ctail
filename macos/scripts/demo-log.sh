#!/usr/bin/env bash
# Append realistic web-server log lines to a file, for demos and App Review recordings.
# usage: demo-log.sh [file]   (default ~/Desktop/ctail-demo.log; Ctrl-C to stop)
set -euo pipefail
OUT="${1:-$HOME/Desktop/ctail-demo.log}"
paths=(/api/v1/health /api/v1/users /api/v1/orders /api/v1/orders/42 /login /static/app.js)
ips=(192.168.1.42 192.168.1.55 10.0.0.7 172.16.4.20)
line() {
  local ts lvl msg r=$((RANDOM % 20))
  ts=$(date '+%Y-%m-%d %H:%M:%S').$(printf '%03d' $((RANDOM % 1000)))
  local ip=${ips[RANDOM % ${#ips[@]}]} path=${paths[RANDOM % ${#paths[@]}]} ms=$((RANDOM % 300 + 3))
  if   [ $r -eq 0 ]; then lvl=ERROR; msg="$ip GET $path 500 ${ms}ms - upstream timeout"
  elif [ $r -le 2 ]; then lvl=WARN;  msg="Slow request from $ip: $path took $((ms + 400))ms"
  elif [ $r -eq 3 ]; then lvl=DEBUG; msg="Cache miss for $path"
  else                    lvl=INFO;  msg="$ip GET $path 200 ${ms}ms"
  fi
  printf '%s [%s] %s\n' "$ts" "$lvl" "$msg"
}
[ -s "$OUT" ] || for _ in $(seq 40); do line; done > "$OUT"   # some history to scroll and search
echo "writing to $OUT"
while :; do line >> "$OUT"; sleep 0.$((RANDOM % 8 + 2)); done
