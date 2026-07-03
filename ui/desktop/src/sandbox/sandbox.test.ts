import { describe, it, expect, afterEach } from 'vitest';
import { buildSandboxProfile, isSandboxEnabled, isSandboxAvailable } from './index';
import {
  normalizeDomain,
  isIPAddress,
  isLoopback,
  matchesBlocked,
  loadBlocked,
  checkBlocked,
} from './proxy';

describe('buildSandboxProfile', () => {
  it('always denies writes to the sandbox config and goose config.yaml', () => {
    const profile = buildSandboxProfile({
      homeDir: '/home/test',
      protectSensitiveFiles: false,
      blockRawSockets: false,
      blockTunnelingTools: false,
    });

    expect(profile).toContain('(deny file-write* (subpath "/home/test/.config/goose/sandbox"))');
    expect(profile).toContain(
      '(deny file-write* (literal "/home/test/.config/goose/config.yaml"))'
    );
    expect(profile).toContain('(deny network*)');
    expect(profile).toContain('(allow network-outbound (remote ip "localhost:*"))');
  });

  it('protects SSH and shell config files when protectSensitiveFiles is true', () => {
    const profile = buildSandboxProfile({
      homeDir: '/home/test',
      protectSensitiveFiles: true,
      blockRawSockets: false,
      blockTunnelingTools: false,
    });

    expect(profile).toContain('(deny file-write* (subpath "/home/test/.ssh"))');
    expect(profile).toContain('(deny file-write* (literal "/home/test/.bashrc"))');
  });

  it('omits sensitive file protection when protectSensitiveFiles is false', () => {
    const profile = buildSandboxProfile({
      homeDir: '/home/test',
      protectSensitiveFiles: false,
      blockRawSockets: false,
      blockTunnelingTools: false,
    });

    expect(profile).not.toContain('.ssh');
    expect(profile).not.toContain('.bashrc');
  });

  it('blocks raw sockets when blockRawSockets is true', () => {
    const profile = buildSandboxProfile({
      homeDir: '/home/test',
      protectSensitiveFiles: false,
      blockRawSockets: true,
      blockTunnelingTools: false,
    });

    expect(profile).toContain('(socket-domain AF_INET)');
    expect(profile).toContain('(socket-domain AF_INET6)');
  });

  it('blocks tunneling tools when blockTunnelingTools is true', () => {
    const profile = buildSandboxProfile({
      homeDir: '/home/test',
      protectSensitiveFiles: false,
      blockRawSockets: false,
      blockTunnelingTools: true,
    });

    expect(profile).toContain('(literal "/usr/bin/nc")');
    expect(profile).toContain('(literal "/usr/bin/socat")');
  });
});

describe('isSandboxEnabled', () => {
  const originalEnv = process.env.GOOSE_SANDBOX;

  afterEach(() => {
    if (originalEnv === undefined) {
      delete process.env.GOOSE_SANDBOX;
    } else {
      process.env.GOOSE_SANDBOX = originalEnv;
    }
  });

  it('is false when unset', () => {
    delete process.env.GOOSE_SANDBOX;
    expect(isSandboxEnabled()).toBe(false);
  });

  it('is true for "true" or "1"', () => {
    process.env.GOOSE_SANDBOX = 'true';
    expect(isSandboxEnabled()).toBe(true);
    process.env.GOOSE_SANDBOX = '1';
    expect(isSandboxEnabled()).toBe(true);
  });

  it('is false for any other value', () => {
    process.env.GOOSE_SANDBOX = 'yes';
    expect(isSandboxEnabled()).toBe(false);
  });
});

describe('isSandboxAvailable', () => {
  it('is false on non-macOS platforms regardless of sandbox-exec presence', () => {
    const originalPlatform = process.platform;
    Object.defineProperty(process, 'platform', { value: 'linux' });
    try {
      expect(isSandboxAvailable()).toBe(false);
    } finally {
      Object.defineProperty(process, 'platform', { value: originalPlatform });
    }
  });
});

describe('normalizeDomain', () => {
  it('lowercases and trims', () => {
    expect(normalizeDomain(' Example.COM ')).toBe('example.com');
  });

  it('strips a trailing dot', () => {
    expect(normalizeDomain('example.com.')).toBe('example.com');
  });

  it('strips brackets from a literal IPv6 address', () => {
    expect(normalizeDomain('[::1]')).toBe('::1');
  });
});

describe('isIPAddress', () => {
  it('recognizes IPv4 addresses', () => {
    expect(isIPAddress('192.168.1.1')).toBe(true);
  });

  it('recognizes IPv6 addresses', () => {
    expect(isIPAddress('::1')).toBe(true);
  });

  it('rejects hostnames', () => {
    expect(isIPAddress('example.com')).toBe(false);
  });
});

describe('isLoopback', () => {
  it('recognizes localhost and loopback addresses', () => {
    expect(isLoopback('localhost')).toBe(true);
    expect(isLoopback('127.0.0.1')).toBe(true);
    expect(isLoopback('::1')).toBe(true);
  });

  it('rejects non-loopback hosts', () => {
    expect(isLoopback('example.com')).toBe(false);
  });
});

describe('matchesBlocked', () => {
  it('matches an exact domain', () => {
    expect(matchesBlocked('evil.com', new Set(['evil.com']))).toBe(true);
  });

  it('matches a subdomain of a blocked parent domain', () => {
    expect(matchesBlocked('api.evil.com', new Set(['evil.com']))).toBe(true);
  });

  it('does not match an unrelated domain', () => {
    expect(matchesBlocked('example.com', new Set(['evil.com']))).toBe(false);
  });
});

describe('loadBlocked', () => {
  it('returns an empty set when no path is given', () => {
    expect(loadBlocked(undefined).size).toBe(0);
  });

  it('returns an empty set when the file does not exist', () => {
    expect(loadBlocked('/nonexistent/blocked.txt').size).toBe(0);
  });
});

describe('checkBlocked', () => {
  it('blocks a raw IP address by default', async () => {
    const result = await checkBlocked('192.168.1.1', 443, new Set(), undefined, undefined, {});
    expect(result.blocked).toBe(true);
    expect(result.reason).toBe('ip-address');
  });

  it('allows a raw IP address when allowIPAddresses is set', async () => {
    const result = await checkBlocked('192.168.1.1', 443, new Set(), undefined, undefined, {
      allowIPAddresses: true,
    });
    expect(result.blocked).toBe(false);
  });

  it('blocks a domain on the blocklist', async () => {
    const result = await checkBlocked(
      'evil.com',
      443,
      new Set(['evil.com']),
      undefined,
      undefined,
      { allowIPAddresses: true }
    );
    expect(result.blocked).toBe(true);
    expect(result.reason).toBe('blocklist');
  });

  it('allows an unlisted domain on a non-SSH port', async () => {
    const result = await checkBlocked('example.com', 443, new Set(), undefined, undefined, {});
    expect(result.blocked).toBe(false);
  });

  it('restricts SSH port 22 to the default git hosts', async () => {
    const blockedNonGit = await checkBlocked(
      'evil.com',
      22,
      new Set(),
      undefined,
      undefined,
      {}
    );
    expect(blockedNonGit.blocked).toBe(true);
    expect(blockedNonGit.reason).toBe('ssh-non-git-host');

    const allowedGit = await checkBlocked(
      'github.com',
      22,
      new Set(),
      undefined,
      undefined,
      {}
    );
    expect(allowedGit.blocked).toBe(false);
  });

  it('blocks SSH entirely when allowSSH is false', async () => {
    const result = await checkBlocked('github.com', 22, new Set(), undefined, undefined, {
      allowSSH: false,
    });
    expect(result.blocked).toBe(true);
    expect(result.reason).toBe('ssh-disabled');
  });

  it('allows SSH to any host when allowSSHToAllHosts is set', async () => {
    const result = await checkBlocked('example.com', 22, new Set(), undefined, undefined, {
      allowSSHToAllHosts: true,
    });
    expect(result.blocked).toBe(false);
  });

  it('blocks loopback only when blockLoopback is set', async () => {
    const blocked = await checkBlocked('localhost', 443, new Set(), undefined, undefined, {
      blockLoopback: true,
      allowIPAddresses: true,
    });
    expect(blocked.blocked).toBe(true);
    expect(blocked.reason).toBe('loopback');

    const allowed = await checkBlocked('localhost', 443, new Set(), undefined, undefined, {
      allowIPAddresses: true,
    });
    expect(allowed.blocked).toBe(false);
  });
});
