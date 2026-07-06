import type { ExternalGoosedConfig } from './settings';

const DEFAULT_CONNECT_SOURCES = [
  "'self'",
  'http://127.0.0.1:*',
  'https://127.0.0.1:*',
  'ws://127.0.0.1:*',
  'wss://127.0.0.1:*',
  'http://localhost:*',
  'https://localhost:*',
  'ws://localhost:*',
  'wss://localhost:*',
  'https://api.github.com',
  'https://github.com',
  'https://objects.githubusercontent.com',
];

export function buildConnectSrc(externalGoosed?: ExternalGoosedConfig): string {
  const sources = [...DEFAULT_CONNECT_SOURCES];

  if (externalGoosed?.enabled && externalGoosed.url) {
    try {
      const externalUrl = new URL(externalGoosed.url);
      sources.push(externalUrl.origin);
      externalUrl.protocol = externalUrl.protocol === 'https:' ? 'wss:' : 'ws:';
      sources.push(externalUrl.origin);
    } catch {
      console.warn('Invalid external goosed URL in settings, skipping CSP entry');
    }
  }

  return sources.join(' ');
}

/**
 * Returns true when upgrade-insecure-requests should be included in the CSP.
 *
 * The directive is omitted when the user has configured an external backend
 * that uses plain HTTP, because Chromium would silently rewrite those
 * requests to HTTPS. The remote server typically does not speak TLS, so the
 * upgraded requests fail with "Failed to fetch".
 *
 * Loopback addresses (127.0.0.1 / localhost) are exempt from the upgrade
 * per the CSP spec, which is why the built-in local backend is unaffected.
 */
export function shouldUpgradeInsecureRequests(externalGoosed?: ExternalGoosedConfig): boolean {
  if (!externalGoosed?.enabled || !externalGoosed.url) {
    return true;
  }

  try {
    const parsed = new URL(externalGoosed.url);
    return parsed.protocol !== 'http:';
  } catch {
    return true;
  }
}

export function buildCSP(externalGoosed?: ExternalGoosedConfig): string {
  const connectSrc = buildConnectSrc(externalGoosed);
  const upgradeDirective = shouldUpgradeInsecureRequests(externalGoosed)
    ? 'upgrade-insecure-requests;'
    : '';

  return (
    "default-src 'self';" +
    // Allow inline styles (used by our React components) and external stylesheets (for MCP-UI)
    "style-src 'self' 'unsafe-inline' https:;" +
    "style-src-elem 'self' 'unsafe-inline' https:;" +
    "script-src 'self' 'unsafe-inline';" +
    "img-src 'self' data: https:;" +
    `connect-src ${connectSrc};` +
    "object-src 'none';" +
    "frame-src 'self' https: http:;" +
    "font-src 'self' data: https:;" +
    // Media sources - allow microphone, https media, and blobs/data URLs (for MCP-UI)
    "media-src 'self' https: blob: data: mediastream:;" +
    "media-src-elem 'self' https: blob: data: mediastream:;" +
    "form-action 'none';" +
    "base-uri 'self';" +
    "manifest-src 'self';" +
    "worker-src 'self';" +
    upgradeDirective
  );
}
