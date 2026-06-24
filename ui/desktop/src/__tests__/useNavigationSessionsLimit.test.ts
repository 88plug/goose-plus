import { describe, it, expect, beforeEach } from 'vitest';
import {
  getRecentSessionsLimit,
  DEFAULT_RECENT_SESSIONS_LIMIT,
  RECENT_SESSIONS_LIMIT_STORAGE_KEY,
} from '../hooks/useNavigationSessions';

describe('getRecentSessionsLimit', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('returns the default when nothing is stored', () => {
    expect(getRecentSessionsLimit()).toBe(DEFAULT_RECENT_SESSIONS_LIMIT);
  });

  it('returns a valid configured value', () => {
    localStorage.setItem(RECENT_SESSIONS_LIMIT_STORAGE_KEY, '10');
    expect(getRecentSessionsLimit()).toBe(10);
  });

  it('falls back to the default for non-numeric values', () => {
    localStorage.setItem(RECENT_SESSIONS_LIMIT_STORAGE_KEY, 'abc');
    expect(getRecentSessionsLimit()).toBe(DEFAULT_RECENT_SESSIONS_LIMIT);
  });

  it('falls back to the default for values below 1', () => {
    localStorage.setItem(RECENT_SESSIONS_LIMIT_STORAGE_KEY, '0');
    expect(getRecentSessionsLimit()).toBe(DEFAULT_RECENT_SESSIONS_LIMIT);
  });
});
