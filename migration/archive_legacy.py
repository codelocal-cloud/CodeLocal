"""One-time source archive for the CodeLocal beta Rust/Next rewrite.

Moves *only Git-tracked files* to old-project, so ignored node_modules,
Next build caches, private env files, and CodeLocal runtime state stay put.
Run on a clean beta working tree created from main.
"""
from __future__ import annotations

import json
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ARCHIVE = ROOT / "old-project"

def git(*args: str) -> str:
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()

def main() -> None:
    if git("branch", "--show-current") != "beta":
        raise SystemExit("Refusing to archive outside beta")
    if git("status", "--porcelain"):
        # Ignore this untracked migration script, nothing else.
        status = git("status", "--porcelain").splitlines()
        if any(line.strip() not in ("?? migration/", "?? migration/archive_legacy.py", "?? target/") for line in status):
            raise SystemExit("Refusing to archive a dirty worktree")
    if ARCHIVE.exists():
        raise SystemExit("old-project already exists; refusing to overwrite it")
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).split(b"\0")
    paths = [Path(p.decode("utf-8")) for p in tracked if p]
    commit = git("rev-parse", "HEAD")
    for relative in paths:
        if relative.is_absolute() or ".." in relative.parts:
            raise SystemExit("Unsafe Git path")
        source = ROOT / relative
        destination = ARCHIVE / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        source.rename(destination)
    (ARCHIVE / "ARCHIVE_MANIFEST.json").write_text(
        json.dumps({"origin": "main", "commit": commit, "tracked_files": len(paths),
                    "status": "historical reference, never deploy directly"},
                   indent=2) + "\n",
        encoding="utf-8",
    )
    print(f"Archived {len(paths)} tracked files from commit {commit[:12]}")

if __name__ == "__main__":
    main()
