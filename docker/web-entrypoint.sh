#!/bin/sh
# Runs at nginx container start (via /docker-entrypoint.d). Writes the runtime
# config the web shim reads, from env, so the image is built once and pointed at
# any backend without a rebuild.
#
#   GOOSE_API_HOST            backend base URL as seen FROM THE BROWSER
#                             (e.g. http://localhost:3000), default http://localhost:3000
#   GOOSE_SERVER__SECRET_KEY  X-Secret-Key the backend expects
#   GOOSE_WEB_VERSION         optional version string shown in the UI
set -eu

api_host="${GOOSE_API_HOST:-http://localhost:3000}"
secret="${GOOSE_SERVER__SECRET_KEY:-}"
version="${GOOSE_WEB_VERSION:-}"

cat > /usr/share/nginx/html/goose-web-config.js <<EOF
// Generated at container start by web-entrypoint.sh — do not edit.
window.__goose_web__ = {
  apiHost: "${api_host}",
  secret: "${secret}",
  version: "${version}",
  config: { GOOSE_API_HOST: "${api_host}" }
};
EOF

echo "goose-web: backend ${api_host} (secret $( [ -n "${secret}" ] && echo set || echo MISSING ))"
