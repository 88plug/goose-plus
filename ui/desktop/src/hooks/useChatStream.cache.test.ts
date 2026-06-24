import { describe, it, expect } from 'vitest';
import { cachedEntryHasHistory } from './useChatStream';
import type { Message, Session } from '../api';

function makeSession(overrides: Partial<Session> = {}): Session {
  return {
    id: 'sess-1',
    name: 'untitled',
    message_count: 0,
    created_at: new Date().toISOString(),
    updated_at: new Date().toISOString(),
    working_dir: '/tmp',
    extension_data: { active: [], installed: [] },
    ...overrides,
  } as Session;
}

function makeMessage(): Message {
  return {
    role: 'user',
    created: Date.now(),
    content: [{ type: 'text', text: 'hi' }],
  } as Message;
}

describe('cachedEntryHasHistory', () => {
  it('rejects an empty snapshot for a session that has messages on disk', () => {
    const session = makeSession({ message_count: 5 });
    expect(cachedEntryHasHistory([], session)).toBe(false);
  });

  it('accepts a snapshot whose messages match a populated session', () => {
    const session = makeSession({ message_count: 5 });
    expect(cachedEntryHasHistory([makeMessage()], session)).toBe(true);
  });

  it('accepts an empty snapshot for a genuinely empty session', () => {
    const session = makeSession({ message_count: 0 });
    expect(cachedEntryHasHistory([], session)).toBe(true);
  });
});
