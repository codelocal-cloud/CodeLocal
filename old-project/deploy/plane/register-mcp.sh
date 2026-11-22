#!/usr/bin/env bash
set -euo pipefail

# Registers environment variable references, never credential values, in CodeLocal.
for name in PLANE_API_KEY PLANE_WORKSPACE_SLUG PLANE_BASE_URL; do
  if [[ -z "${!name:-}" ]]; then
    echo "FAIL: export $name in this terminal first. Do not store secrets in Git." >&2
    exit 2
  fi
done
command -v codelocal >/dev/null || { echo "CodeLocal CLI is required." >&2; exit 2; }
command -v uvx >/dev/null || { echo "uvx is required (Python 3.10+)." >&2; exit 2; }

bash "$(dirname "$0")/verify.sh" "$PLANE_BASE_URL"
codelocal mcp add plane --global --no-probe \
  --env PLANE_API_KEY \
  --env PLANE_WORKSPACE_SLUG \
  --env PLANE_BASE_URL \
  -- uvx plane-mcp-server stdio

codelocal mcp probe plane
echo "Plane MCP is ready for this authorized CodeLocal device. Keep the device online for stdio."
