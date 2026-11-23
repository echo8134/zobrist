#!/usr/bin/env bash
# The quality gate. Usage: scripts/check.sh [--all]. Exit 0 is green, exit 2 is red.
# Exit 2 is the code that a Claude Code Stop hook reads as "send the agent back".
set -u

# Re-run under `timeout` so that a hung cargo process fails the gate instead of blocking it.
if [ -z "${CHECK_SH_WATCHDOG:-}" ]; then
  CHECK_SH_WATCHDOG=1 timeout --kill-after=10 540 "$0" "$@"
  status=$?
  [ "$status" -eq 0 ] && exit 0
  [ "$status" -eq 124 ] && echo "check.sh: timed out, treat as failed" >&2
  exit 2
fi

# A Stop hook passes JSON on stdin. `stop_hook_active` means that the hook already sent the
# agent back once. Let that stop through, or a red gate would loop. The two-second limit keeps
# a caller that never closes stdin from hanging the gate.
if [ ! -t 0 ]; then
  hook_input=""
  read -r -t 2 -d '' hook_input || true
  case "$hook_input" in
    *'"stop_hook_active": true'* | *'"stop_hook_active":true'*) exit 0 ;;
  esac
fi

cd "$(git rev-parse --show-toplevel)" || exit 2

# Without --all, skip the run when no file that the gate checks differs from HEAD. That keeps
# the Stop hook cheap. It also means that the hook never checks a commit, so CI runs --all.
paths=('*.rs' '*.toml' '*.md' 'Cargo.lock' 'scripts/*' 'crates/app/*')
if [ "${1:-}" != "--all" ]; then
  if git rev-parse --verify --quiet HEAD >/dev/null \
    && git diff --quiet HEAD -- "${paths[@]}" \
    && [ -z "$(git ls-files --others --exclude-standard -- "${paths[@]}")" ]; then
    exit 0
  fi
fi

view=crates/app/view

need() {
  command -v "$1" >/dev/null 2>&1 && return
  echo "check.sh: $1 is missing. $2" >&2
  exit 2
}
need cargo-nextest "Install with: cargo install --locked cargo-nextest"
need cargo-deny "Install with: cargo install --locked cargo-deny"
need jq "Install it with the system package manager"
need node "Install Node.js $(jq --raw-output .engines.node "$view/package.json")"
need pnpm "Install with: npm install --global $(jq --raw-output .packageManager "$view/package.json")"
need docker "Install Docker. The end-to-end test runs its display in a container"
if ! docker info >/dev/null 2>&1; then
  echo "check.sh: Docker is not running. Start it. The end-to-end test runs its display in a container" >&2
  exit 2
fi

# Record a failure and keep going, so that one run reports every red step.
failed=0
run() {
  echo "+ $*" >&2
  "$@" || failed=1
}

run cargo fmt --all --check
# zobrist-app embeds the built view when it compiles, so the view comes before every cargo
# step that compiles the workspace.
run pnpm --dir "$view" install --frozen-lockfile
run pnpm --dir "$view" run build
run pnpm --dir "$view" run lint
run pnpm --dir "$view" run typecheck
run pnpm --dir "$view" run test
run cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
run scripts/check-deps.sh
run scripts/check-docs.sh
run python3 scripts/test-release-notes.py
# The test run below rewrites the generated types, so only a check before it can find them stale.
run scripts/check-generated.sh
# nextest fails a run that finds no tests unless --no-tests=pass is set.
run cargo nextest run --workspace --all-features --locked --no-tests=pass
# nextest does not run doc tests.
run cargo test --workspace --doc --locked
run cargo deny --locked --all-features check
run scripts/check-e2e.sh

if [ "$failed" -ne 0 ]; then
  echo "check.sh: failed, fix the cause before finishing" >&2
  exit 2
fi
exit 0
