#!/usr/bin/env bash
# Checks that the TypeScript types of the view match the Rust types that they come from.
# Usage: scripts/check-generated.sh. Exit 0 when the checked-in types are current.
set -u -o pipefail
cd "$(git rev-parse --show-toplevel)" || exit 2

# ts-rs writes the types when the tests of zobrist-app run. Here they go into a fresh folder,
# and the working tree stays as it is.
rm -rf target/generated
TS_RS_EXPORT_DIR="$PWD/target/generated" \
  cargo test --quiet -p zobrist-app --lib --all-features --locked export_bindings || exit 1

if ! diff --recursive target/generated crates/app/view/src/generated; then
  echo "check-generated: crates/app/view/src/generated is stale. Regenerate with: cargo test -p zobrist-app --lib export_bindings" >&2
  exit 1
fi
