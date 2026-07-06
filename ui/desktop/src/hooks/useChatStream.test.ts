import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { ChatState } from '../types/chatState';
import type { Message } from '../api';
import type { SessionEvent } from './useSessionEvents';

// useChatStream pulls in a wide module graph (electron preload, i18n, api
// client). We only exercise the pure event processor here, so stub those out.
vi.mock('../api', () => ({}));
vi.mock('../i18n', () => ({
  defineMessages: (m: unknown) => m,
  useIntl: () => ({ formatMessage: () => '' }),
}));

import {
  createEventProcessor,
  clearSessionCache,
  getCachedSession,
  messageWeight,
  resultsCacheSet,
} from './useChatStream';
import type { Session } from '../api';

function assistantMessage(text: string): Message {
  return {
    role: 'assistant',
    created: Date.now(),
    id: 'assistant-1',
    content: [{ type: 'text', text }],
    metadata: { agentVisible: true, userVisible: true },
  };
}

function toolMessage(type: 'toolRequest' | 'toolResponse'): Message {
  return {
    role: 'assistant',
    created: Date.now(),
    id: `${type}-1`,
    content: [{ type } as unknown as Message['content'][number]],
    metadata: { agentVisible: true, userVisible: true },
  };
}

function fakeSession(id: string): Session {
  return { id, message_count: 0 } as unknown as Session;
}

function messageEvent(message: Message): SessionEvent {
  return { type: 'Message', message, token_state: undefined } as unknown as SessionEvent;
}

function setReducedMotion(reduce: boolean) {
  window.matchMedia = vi.fn().mockImplementation((query: string) => ({
    matches: query.includes('reduce') ? reduce : false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })) as unknown as typeof window.matchMedia;
}

describe('createEventProcessor reduced-motion flush (#8997)', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('flushes a batched reply via a timer when no further event arrives', () => {
    setReducedMotion(true);
    const dispatch = vi.fn();
    const onFinish = vi.fn();

    const processEvent = createEventProcessor([], dispatch, onFinish, 'session-1');

    // A single text delta arrives within the batch interval, so it is buffered
    // rather than dispatched immediately.
    processEvent(messageEvent(assistantMessage('hello')));

    const messagesDispatchedImmediately = dispatch.mock.calls.some(
      ([action]) => action.type === 'SET_MESSAGES'
    );
    expect(messagesDispatchedImmediately).toBe(false);

    // Before the fix the reply would only surface after a remount. The flush
    // timer must push it to the UI once the batch interval elapses.
    vi.advanceTimersByTime(1000);

    const setMessages = dispatch.mock.calls
      .map(([action]) => action)
      .find((action) => action.type === 'SET_MESSAGES');
    expect(setMessages).toBeDefined();
    const payload = (setMessages as { payload: Message[] }).payload;
    expect(payload[payload.length - 1].content[0]).toMatchObject({
      type: 'text',
      text: 'hello',
    });
  });

  it('dispatches each event immediately when reduced motion is off', () => {
    setReducedMotion(false);
    const dispatch = vi.fn();
    const onFinish = vi.fn();

    const processEvent = createEventProcessor([], dispatch, onFinish, 'session-1');
    processEvent(messageEvent(assistantMessage('hello')));

    const setMessages = dispatch.mock.calls
      .map(([action]) => action)
      .find((action) => action.type === 'SET_MESSAGES');
    expect(setMessages).toBeDefined();
  });

  it('clears the pending flush timer on the terminal Finish event', () => {
    setReducedMotion(true);
    const dispatch = vi.fn();
    const onFinish = vi.fn();
    const clearSpy = vi.spyOn(globalThis, 'clearTimeout');

    const processEvent = createEventProcessor([], dispatch, onFinish, 'session-1');
    processEvent(messageEvent(assistantMessage('hello')));

    const terminal = processEvent({ type: 'Finish' } as unknown as SessionEvent);
    expect(terminal).toBe(true);
    expect(onFinish).toHaveBeenCalled();
    expect(clearSpy).toHaveBeenCalled();

    // After Finish there must be no lingering timer that fires a stale flush.
    dispatch.mockClear();
    vi.advanceTimersByTime(2000);
    const lateFlush = dispatch.mock.calls.some(([action]) => action.type === 'SET_MESSAGES');
    expect(lateFlush).toBe(false);
  });
});

describe('chat state constant sanity', () => {
  it('exposes the Streaming state used while batching', () => {
    expect(ChatState.Streaming).toBeDefined();
  });
});

describe('resultsCache bounding (prevent unbounded growth over long sessions)', () => {
  it('weighs tool messages 3x a plain text message', () => {
    expect(messageWeight([assistantMessage('hi')])).toBe(1);
    expect(messageWeight([toolMessage('toolRequest')])).toBe(3);
    expect(messageWeight([toolMessage('toolResponse')])).toBe(3);
    expect(messageWeight([assistantMessage('hi'), toolMessage('toolRequest')])).toBe(4);
  });

  it('refuses to cache a conversation heavier than the entry weight limit', () => {
    const sessionId = 'weight-limit-test';
    const heavyMessages = Array.from({ length: 70 }, () => toolMessage('toolRequest')); // weight 210 > 200
    resultsCacheSet(sessionId, { session: fakeSession(sessionId), messages: heavyMessages });
    expect(getCachedSession(sessionId)).toBeUndefined();
  });

  it('caches a conversation at or under the entry weight limit', () => {
    const sessionId = 'weight-ok-test';
    const okMessages = Array.from({ length: 60 }, () => toolMessage('toolRequest')); // weight 180 <= 200
    resultsCacheSet(sessionId, { session: fakeSession(sessionId), messages: okMessages });
    expect(getCachedSession(sessionId)).toBeDefined();
    clearSessionCache(sessionId);
  });

  it('evicts the oldest entry once more than MAX_CACHED_SESSIONS are held', () => {
    const ids = Array.from({ length: 6 }, (_, i) => `evict-test-${i}`);
    for (const id of ids) {
      resultsCacheSet(id, { session: fakeSession(id), messages: [assistantMessage('hi')] });
    }
    // Cache is capped at 5 — the first (oldest) of the 6 inserted must be gone.
    expect(getCachedSession(ids[0])).toBeUndefined();
    expect(getCachedSession(ids[5])).toBeDefined();
    for (const id of ids) {
      clearSessionCache(id);
    }
  });
});
