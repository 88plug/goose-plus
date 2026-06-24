// Overwritten at deploy time (e.g. by the Docker entrypoint) to inject the
// goosed backend URL + secret. Safe default for local dev.
window.__goose_web__ = window.__goose_web__ || {};
