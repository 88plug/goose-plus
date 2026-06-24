interface GooseWebRuntime {
  apiHost: string;
  secret: string;
  version: string;
  config: Record<string, unknown>;
}

declare global {
  interface Window {
    __goose_web__?: Partial<GooseWebRuntime>;
  }
}

function readInjected(): Partial<GooseWebRuntime> {
  return window.__goose_web__ ?? {};
}

function readQueryParams(): { apiHost?: string; secret?: string } {
  try {
    const params = new URLSearchParams(window.location.search);
    const apiHost = params.get('apiHost') ?? undefined;
    const secret = params.get('secret') ?? undefined;
    return { apiHost, secret };
  } catch {
    return {};
  }
}

function resolveRuntime(): GooseWebRuntime {
  const injected = readInjected();
  const query = readQueryParams();

  const apiHost = query.apiHost || injected.apiHost || window.location.origin;
  const secret = query.secret || injected.secret || '';
  const version = injected.version || '';
  const config = injected.config ?? {};

  return { apiHost, secret, version, config };
}

const resolved = resolveRuntime();

export const apiHost: string = resolved.apiHost;
export const secret: string = resolved.secret;
export const version: string = resolved.version;
export const config: Record<string, unknown> = resolved.config;
