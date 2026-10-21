# @file CT-DEV-005
# Installs a commit-msg hook that runs `clew check-commit` on every commit message.
# Git for Windows runs hooks with its bundled sh, so the hook is a POSIX shell script.
# Run from anywhere inside the repository: powershell -File scripts\install-hooks.ps1
$ErrorActionPreference = 'Stop'

$hooksDir = (git rev-parse --git-path hooks)
if ($LASTEXITCODE -ne 0) { throw 'Not inside a git repository.' }
New-Item -ItemType Directory -Force -Path $hooksDir | Out-Null
$hook = Join-Path (Resolve-Path $hooksDir) 'commit-msg'

$lines = @(
    '#!/bin/sh'
    '# Installed by scripts/install-hooks.ps1. Checks the Refs: trailer (CT-DEV-005).'
    '# Uses $CLEW if set, then `clew` on PATH, then `cargo run` from this repository.'
    'if [ -n "${CLEW:-}" ]; then'
    '    exec "$CLEW" check-commit "$1"'
    'elif command -v clew >/dev/null 2>&1; then'
    '    exec clew check-commit "$1"'
    'elif command -v cargo >/dev/null 2>&1; then'
    '    exec cargo run --quiet -p clew-cli -- check-commit "$1"'
    'fi'
    'echo "commit-msg: clew not found. Install it or put cargo on PATH." >&2'
    'exit 1'
)
# LF line endings and no byte order mark, so sh can run it.
[System.IO.File]::WriteAllText($hook, (($lines -join "`n") + "`n"), (New-Object System.Text.UTF8Encoding $false))
Write-Output "Installed $hook"
