import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import ToolCallWithResponse from './ToolCallWithResponse';
import { IntlTestWrapper } from '../i18n/test-utils';
import { ToolRequestMessageContent, ToolResponseMessageContent } from '../types/message';

function makeRequest(name: string, args: Record<string, unknown> = {}): ToolRequestMessageContent {
  return {
    type: 'toolRequest',
    id: 'req-1',
    toolCall: { status: 'success', value: { name, arguments: args } },
  } as unknown as ToolRequestMessageContent;
}

function makeEmptyResponse(): ToolResponseMessageContent {
  return {
    type: 'toolResponse',
    id: 'req-1',
    toolResult: { status: 'success', value: { content: [] } },
  } as unknown as ToolResponseMessageContent;
}

function makeTextResponse(text: string): ToolResponseMessageContent {
  return {
    type: 'toolResponse',
    id: 'req-1',
    toolResult: { status: 'success', value: { content: [{ type: 'text', text }] } },
  } as unknown as ToolResponseMessageContent;
}

describe('ToolCallWithResponse expand affordance', () => {
  it('disables the expand button and hides the chevron when there is nothing to show', () => {
    render(
      <IntlTestWrapper>
        <ToolCallWithResponse
          isCancelledMessage={false}
          toolRequest={makeRequest('final_output')}
          toolResponse={makeEmptyResponse()}
          isPendingApproval={false}
        />
      </IntlTestWrapper>
    );

    const button = screen.getByRole('button');
    expect(button).toBeDisabled();
    // Only the tool-status icon should render — no ChevronRight expand affordance.
    expect(button.querySelectorAll('svg').length).toBe(1);
  });

  it('keeps the expand button enabled with a chevron when the tool has output', () => {
    render(
      <IntlTestWrapper>
        <ToolCallWithResponse
          isCancelledMessage={false}
          toolRequest={makeRequest('shell', { command: 'ls' })}
          toolResponse={makeTextResponse('file1\nfile2')}
          isPendingApproval={false}
        />
      </IntlTestWrapper>
    );

    const button = screen.getByRole('button');
    expect(button).not.toBeDisabled();
    // Tool-status icon + ChevronRight expand affordance.
    expect(button.querySelectorAll('svg').length).toBe(2);
  });
});
