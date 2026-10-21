#!/bin/sh
# @file CT-DEV-005
# Installs a commit-msg hook that runs `clew check-commit` on every commit message.
# Run from anywhere inside the repository: sh scripts/install-hooks.sh
set -eu

hooks_dir=$(git rev-parse --git-path hooks)
mkdir -p "$hooks_dir"
hook="$hooks_dir/commit-msg"

cat > "$hook" <<'HOOK'
#!/bin/sh
# Installed by scripts/install-hooks.sh. Checks the Refs: trailer (CT-DEV-005).
# Uses $CLEW if set, then `clew` on PATH, then `cargo run` from this repository.
if [ -n "${CLEW:-}" ]; then
    exec "$CLEW" check-commit "$1"
elif command -v clew >/dev/null 2>&1; then
    exec clew check-commit "$1"
elif command -v cargo >/dev/null 2>&1; then
    exec cargo run --quiet -p clew-cli -- check-commit "$1"
fi
echo "commit-msg: clew not found. Install it or put cargo on PATH." >&2
exit 1
HOOK
chmod +x "$hook"
echo "Installed $hook"
