#!/usr/bin/env bash
set -euo pipefail

# Fail closed before downloading a multi-service Plane stack.
failed=0
if ! command -v docker >/dev/null 2>&1; then
  echo "FAIL: Docker Engine is not installed." >&2
  failed=1
elif ! docker info >/dev/null 2>&1; then
  echo "FAIL: Docker Engine is not running or this user lacks access." >&2
  failed=1
else
  if ! docker compose version >/dev/null 2>&1; then
    echo "FAIL: Docker Compose v2+ is required." >&2
    failed=1
  fi
  bytes="$(docker info --format '{{.MemTotal}}')"
  if [[ ! "$bytes" =~ ^[0-9]+$ ]]; then
    echo "FAIL: cannot read Docker memory allocation." >&2
    failed=1
  elif (( bytes < 4 * 1024 * 1024 * 1024 )); then
    echo "FAIL: Docker has less than 4 GiB RAM." >&2
    failed=1
  elif (( bytes < 8 * 1024 * 1024 * 1024 )); then
    echo "WARN: Docker has less than the 8 GiB recommended for production."
  else
    echo "PASS: Docker RAM allocation >= 8 GiB."
  fi
fi

free_kib="$(df -Pk . | awk 'END {print $4}')"
if [[ ! "$free_kib" =~ ^[0-9]+$ ]]; then
  echo "FAIL: cannot read available disk space." >&2
  failed=1
elif (( free_kib < 20 * 1024 * 1024 )); then
  echo "FAIL: less than 20 GiB free disk space. Do not pull the Plane stack on this host." >&2
  failed=1
else
  echo "PASS: at least 20 GiB disk space available."
fi

if (( failed != 0 )); then
  echo "Plane installation blocked. Use a dedicated server; do not prune existing Docker volumes automatically." >&2
  exit 1
fi
echo "PASS: host meets baseline prerequisites. Confirm TLS, backups, SMTP and resource quotas before production."
