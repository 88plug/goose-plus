import React from 'react';
import { defineMessages, useIntl } from '../../i18n';
import { Message } from '../../api';
import { getTextAndImageContent, getThinkingContent } from '../../types/message';
import { ListFilter } from 'lucide-react';
import { Button } from '../ui/button';

const i18n = defineMessages({
  filterToMatches: {
    id: 'conversationFilter.filterToMatches',
    defaultMessage: 'Show only matches',
  },
  showAll: {
    id: 'conversationFilter.showAll',
    defaultMessage: 'Show all messages',
  },
  matchSummary: {
    id: 'conversationFilter.matchSummary',
    defaultMessage: '{matched} of {total} messages match',
  },
  noMatches: {
    id: 'conversationFilter.noMatches',
    defaultMessage: 'No messages match "{term}"',
  },
});

/**
 * Returns the searchable plain text for a message: rendered text content plus
 * any thinking content. Tool-call markup is already stripped by
 * getTextAndImageContent for assistant messages.
 */
export function getMessageSearchText(message: Message): string {
  const { textContent } = getTextAndImageContent(message);
  const thinking = getThinkingContent(message) ?? '';
  return `${textContent}\n${thinking}`;
}

/**
 * Filters messages to those whose searchable text contains the term. An empty
 * term returns the original list unchanged so callers can pass through safely.
 */
export function filterMessagesBySearch(
  messages: Message[],
  term: string,
  caseSensitive: boolean
): Message[] {
  const trimmed = term.trim();
  if (!trimmed) {
    return messages;
  }
  const needle = caseSensitive ? trimmed : trimmed.toLowerCase();
  return messages.filter((message) => {
    const haystack = getMessageSearchText(message);
    return (caseSensitive ? haystack : haystack.toLowerCase()).includes(needle);
  });
}

interface ConversationFilterProps {
  /** The active search term driving the filter. */
  term: string;
  /** Whether the filter (collapse non-matching messages) is enabled. */
  enabled: boolean;
  /** Toggle the collapse-non-matching-messages behavior. */
  onToggle: (enabled: boolean) => void;
  /** Number of messages matching the term. */
  matchedCount: number;
  /** Total number of messages in the conversation. */
  totalCount: number;
}

/**
 * ConversationFilter renders a compact toggle that lets the user collapse the
 * conversation down to only the messages matching the active find term
 * (issue #1505). It is intended to sit just above the message list and only
 * renders while a search term is active.
 */
export const ConversationFilter: React.FC<ConversationFilterProps> = ({
  term,
  enabled,
  onToggle,
  matchedCount,
  totalCount,
}) => {
  const intl = useIntl();

  if (!term.trim()) {
    return null;
  }

  const summary =
    matchedCount === 0
      ? intl.formatMessage(i18n.noMatches, { term: term.trim() })
      : intl.formatMessage(i18n.matchSummary, { matched: matchedCount, total: totalCount });

  return (
    <div className="sticky top-0 z-20 mb-3 flex items-center justify-between rounded-md border border-border-subtle bg-background-muted px-3 py-1.5 text-xs text-text-muted">
      <span className="truncate">{summary}</span>
      <Button
        variant="ghost"
        size="sm"
        onClick={() => onToggle(!enabled)}
        className="ml-2 flex h-7 flex-shrink-0 items-center gap-1.5 px-2 text-xs"
        title={intl.formatMessage(enabled ? i18n.showAll : i18n.filterToMatches)}
      >
        <ListFilter className="h-3.5 w-3.5" />
        {intl.formatMessage(enabled ? i18n.showAll : i18n.filterToMatches)}
      </Button>
    </div>
  );
};

export default ConversationFilter;
