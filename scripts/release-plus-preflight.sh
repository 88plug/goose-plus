#!/bin/bash
# Validates goose-plus release invariants before tagging plus-v*.
# Catches the class of failures seen in plus-v1.39.18/.19:
#   - bundle path / productName mismatches (macOS smoke + artifact upload)
#   - stale upstream defaults in desktop updater config
#   - compile errors that preflight gates on
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

BUNDLE_NAME="goose-plus"
ERRORS=0

error() {
  echo "ERROR: $*" >&2
  ERRORS=$((ERRORS + 1))
}

echo "== Release-plus preflight =="

# --- Desktop bundle naming (plus-v1.39.19 root cause) ---
PRODUCT_NAME="$(node -p "require('./ui/desktop/package.json').productName")"
if [[ "$PRODUCT_NAME" != "$BUNDLE_NAME" ]]; then
  error "ui/desktop/package.json productName=$PRODUCT_NAME, expected $BUNDLE_NAME"
fi

for wf in .github/workflows/bundle-desktop.yml .github/workflows/bundle-desktop-intel.yml; do
  if grep -qE 'out/Goose-darwin|/Goose\.zip|Goose\.app' "$wf"; then
    error "$wf still references legacy Goose bundle paths"
  fi
  if ! grep -q "goose-plus-darwin" "$wf"; then
    error "$wf missing goose-plus-darwin upload path"
  fi
  if ! grep -q 'GOOSE_BUNDLE_NAME: "goose-plus"' "$wf"; then
    error "$wf missing GOOSE_BUNDLE_NAME env"
  fi
done

# --- Runtime updater defaults baked in at Vite build time ---
if grep -qE "GOOSE_BUNDLE_NAME.*'Goose'|GITHUB_OWNER.*aaif-goose|GITHUB_REPO.*'goose'" ui/desktop/vite.main.config.mts; then
  error "vite.main.config.mts still has upstream Goose defaults"
fi

if grep -q 'Goose\.app' ui/desktop/src/utils/autoUpdater.ts; then
  error "autoUpdater.ts still hardcodes Goose.app in user-facing text"
fi

# --- Optional Linux artifacts must not hard-fail the annotation gate ---
while IFS= read -r line; do
  file="${line%%:*}"
  if grep -A5 'flatpak' "$file" | grep -q 'if-no-files-found: error'; then
    error "$file: flatpak upload uses if-no-files-found: error (use ignore)"
  fi
done < <(grep -l 'flatpak' .github/workflows/bundle-desktop-linux.yml)

# --- release-plus.yml must glob goose-plus zips ---
if ! grep -q 'goose-plus\*\.zip' .github/workflows/release-plus.yml; then
  error "release-plus.yml missing goose-plus*.zip artifact glob"
fi

# --- Rust preflight (same feature surface as CI, minus local-inference) ---
echo "== Cargo preflight (CI-equivalent) =="
source ./bin/activate-hermit
cargo check -p goose-cli --no-default-features \
  --features code-mode,tui,update,aws-providers,telemetry,nostr,otel,system-keyring,rustls-tls \
  --all-targets
cargo check -p goose-server --no-default-features --features rustls-tls --all-targets

echo "== Nebius provider tests =="
cargo test -p goose nebius -- --nocapture

if [[ "$ERRORS" -gt 0 ]]; then
  echo ""
  echo "Preflight failed with $ERRORS error(s)."
  exit 1
fi

echo ""
echo "Preflight passed."