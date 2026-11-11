#!/usr/bin/env bash
set -euo pipefail

base_url="${1:-${PLANE_BASE_URL:-}}"
if [[ -z "$base_url" ]]; then
  echo "Usage: PLANE_BASE_URL=https://plane.codelocal.cloud bash deploy/plane/verify.sh" >&2
  exit 2
fi
base_url="${base_url%/}"
case "$base_url" in
  https://*) ;;
  http://localhost*|http://127.0.0.1:*) ;;
  *) echo "FAIL: Plane must use HTTPS except for localhost." >&2; exit 2 ;;
esac

command -v curl >/dev/null || { echo "curl is required." >&2; exit 2; }
code="$(curl --silent --show-error --location --max-time 20 --output /dev/null --write-out '%{http_code}' "$base_url/")"
case "$code" in
  200) echo "PASS: Plane website returned HTTP 200." ;;
  *) echo "FAIL: Plane website returned HTTP $code." >&2; exit 1 ;;
esac

code="$(curl --silent --show-error --max-time 20 --output /dev/null --write-out '%{http_code}' "$base_url/api/instances/")"
case "$code" in
  200) echo "PASS: Plane instance API returned HTTP 200." ;;
  *) echo "FAIL: Plane instance API returned HTTP $code." >&2; exit 1 ;;
esac

if command -v python3 >/dev/null 2>&1; then
  setup_done="$(curl --silent --show-error --max-time 20 "$base_url/api/instances/" | python3 -c 'import json,sys; d=json.load(sys.stdin); print("true" if d.get("instance", {}).get("is_setup_done") is True else "false")')"
  if [[ "$setup_done" == "true" ]]; then
    echo "PASS: Plane admin setup is complete."
  else
    echo "FAIL: Plane instance admin setup is incomplete." >&2
    exit 1
  fi
else
  echo "NOT RUN: Plane admin setup JSON check (python3 is unavailable)."
fi

if [[ -n "${PLANE_API_KEY:-}" ]]; then
  code="$(curl --silent --show-error --max-time 20 --output /dev/null --write-out '%{http_code}' -H "x-api-key: $PLANE_API_KEY" "$base_url/api/v1/users/me/")"
  case "$code" in
    200) echo "PASS: Plane REST API authenticated as the token owner." ;;
    *) echo "FAIL: Plane REST API returned HTTP $code. Check account token, base URL and setup." >&2; exit 1 ;;
  esac
else
  echo "NOT RUN: REST API authentication (PLANE_API_KEY is unset)."
fi
