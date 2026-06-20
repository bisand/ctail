#!/usr/bin/env bash
# Demo log generator — writes a growing log file so you can watch the POC tail
# it live (and exercise the highlight rules). Usage: ./gen-log.sh [path] [delay]
set -euo pipefail
FILE="${1:-/tmp/ctail-demo.log}"
DELAY="${2:-0.3}"
LEVELS=(INFO DEBUG WARN ERROR INFO INFO DEBUG)
MSGS=(
  "request handled in 14ms"
  "connecting to https://api.example.com/v1/events"
  "cache miss for key user:42"
  "retrying upstream after timeout"
  "FATAL panic: nil pointer dereference"
  "user logged in from 10.0.0.7"
  "disk usage at 81%"
)
echo "Writing to $FILE (Ctrl-C to stop)"
: > "$FILE"
i=0
while true; do
  lvl=${LEVELS[$((RANDOM % ${#LEVELS[@]}))]}
  msg=${MSGS[$((RANDOM % ${#MSGS[@]}))]}
  ts=$(date "+%Y-%m-%d %H:%M:%S")
  printf '%s %-5s [worker-%d] %s\n' "$ts" "$lvl" "$((i % 4))" "$msg" >> "$FILE"
  i=$((i + 1))
  sleep "$DELAY"
done
