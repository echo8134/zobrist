#!/usr/bin/env bash
# Enforces the dependency direction that ARCHITECTURE.md describes. The lists below are the rule.
# Usage: scripts/check-deps.sh. Exit 0 when every direct dependency is allowed.
set -u -o pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

# Every crate that the workspace may contain. A crate may be listed before it exists.
workspace_crates="
zobrist-core
zobrist-store
zobrist-engine
zobrist-cli
zobrist-app
"
# Allowed edges, one per line: a crate, then a workspace crate that it may depend on.
internal_edges="
zobrist-store zobrist-core
zobrist-engine zobrist-core
zobrist-cli zobrist-core
zobrist-cli zobrist-store
zobrist-cli zobrist-engine
zobrist-app zobrist-core
zobrist-app zobrist-store
zobrist-app zobrist-engine
"
# External crates that zobrist-core may depend on, one per line. Empty means none.
core_allowed=""
owner_of_rusqlite=zobrist-store
owner_of_tauri=zobrist-app

# --no-deps limits the check to direct dependencies. A null kind is a normal dependency.
# Dev-dependencies are left out so that tests may use what they need.
metadata=$(cargo metadata --format-version 1 --locked --no-deps) || exit 2
members=$(jq -r '.packages[].name' <<<"$metadata") || exit 2
edges=$(jq -r '.packages[] | .name as $n | .dependencies[] | select(.kind == null or .kind == "build") | "\($n) \(.name)"' <<<"$metadata") || exit 2

failed=0
fail() {
  echo "check-deps: $*" >&2
  failed=1
}

while read -r member; do
  [ -z "$member" ] && continue
  grep -qxF "$member" <<<"$workspace_crates" \
    || fail "$member is not a known workspace crate. Add it to scripts/check-deps.sh and ARCHITECTURE.md"
done <<<"$members"

while read -r crate dep; do
  [ -z "$crate" ] && continue
  case "$dep" in
    zobrist-*)
      grep -qxF "$dep" <<<"$members" || fail "$crate depends on $dep, which is not a workspace crate"
      grep -qx "$crate $dep" <<<"$internal_edges" || fail "$crate may not depend on $dep" ;;
    rusqlite)
      [ "$crate" = "$owner_of_rusqlite" ] || fail "only $owner_of_rusqlite may depend on $dep, not $crate" ;;
    tauri | tauri-*)
      [ "$crate" = "$owner_of_tauri" ] || fail "only $owner_of_tauri may depend on $dep, not $crate" ;;
  esac
  if [ "$crate" = zobrist-core ]; then
    case "$dep" in
      zobrist-*) ;;
      *) grep -qxF "$dep" <<<"$core_allowed" \
           || fail "zobrist-core may not depend on $dep. Add it to core_allowed in scripts/check-deps.sh after an ADR or design note justifies it" ;;
    esac
  fi
done <<<"$edges"

exit "$failed"
