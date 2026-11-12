#!/usr/bin/env bash
# Checks every path that a document names. A path is a backticked token that contains a slash
# or ends in a known file extension. Usage: scripts/check-docs.sh.
set -u -o pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

failed=0
for doc in README.md AGENTS.md ARCHITECTURE.md docs/adr/*.md; do
  [ -f "$doc" ] || continue
  while IFS=: read -r lineno line; do
    # A path is either there or marked as planned, never both. Without the second half, the
    # word planned would stay on a line long after the path was built.
    planned=0
    case "$line" in *planned*) planned=1 ;; esac
    while read -r ref; do
      [ -z "$ref" ] && continue
      ref=${ref%/}
      if [ "$planned" -eq 1 ]; then
        [ -e "$ref" ] && { echo "check-docs: $doc:$lineno says planned, but $ref exists. Drop the word or move the path to another line" >&2; failed=1; }
      else
        [ -e "$ref" ] || { echo "check-docs: $doc:$lineno names $ref, which does not exist" >&2; failed=1; }
      fi
    done < <(grep -oE '`[A-Za-z0-9_./-]+`' <<<"$line" | tr -d '`' | grep -E '/|\.(md|sh|toml|rs|yml|json|lock)$' | grep -vE '^(https?:|\.\.?$)' || true)
  done < <(grep -n '`' "$doc" || true)
done
exit "$failed"
