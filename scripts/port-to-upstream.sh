#!/bin/bash
# Start a port/* branch from main (upstream mirror) for PRs to aaif-goose/goose.
#
# Usage:
#   ./scripts/port-to-upstream.sh nebius
#   ./scripts/port-to-upstream.sh my-feature --sync-main
#
# Never branch from goose-plus for upstream PRs — the diff would include all plus work.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FEATURE="${1:-}"
SYNC_MAIN=false
for arg in "$@"; do
  if [ "$arg" = "--sync-main" ]; then
    SYNC_MAIN=true
  fi
done

if [ -z "$FEATURE" ] || [ "$FEATURE" = "--sync-main" ]; then
  echo "Usage: $0 <feature-name> [--sync-main]" >&2
  echo "Example: $0 nebius" >&2
  exit 1
fi

BRANCH="port/${FEATURE}"

if ! git remote get-url upstream &>/dev/null; then
  git remote add upstream https://github.com/aaif-goose/goose.git
fi

git fetch upstream main
git fetch origin main 2>/dev/null || true

if $SYNC_MAIN; then
  echo "Syncing origin/main from upstream..."
  git checkout -B main upstream/main
  git push origin main
fi

if git show-ref --verify --quiet "refs/heads/${BRANCH}"; then
  echo "Branch ${BRANCH} already exists locally." >&2
  echo "  git checkout ${BRANCH}" >&2
  exit 1
fi

git checkout -b "$BRANCH" upstream/main

echo ""
echo "Created ${BRANCH} from upstream/main ($(git rev-parse --short HEAD))"
echo ""
echo "Next steps:"
echo "  1. Apply the minimal upstream-ready patch (not the whole goose-plus commit)"
echo "  2. cargo test -p goose <feature>"
echo "  3. git push -u origin ${BRANCH}"
echo "  4. Open PR: https://github.com/aaif-goose/goose/compare/main...88plug:goose-plus:${BRANCH}"
echo ""
echo "See GOOSE_PLUS.md § Upstream ports for the branch model."