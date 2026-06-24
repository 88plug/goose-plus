import { AppEvents } from '../constants/events';
import { AppRenderer, type RequestHandlerExtra } from '@mcp-ui/client';
import type {
  McpUiMessageRequest,
  McpUiMessageResult,
  McpUiOpenLinkRequest,
  McpUiOpenLinkResult,
  McpUiSizeChangedNotification,
} from '@modelcontextprotocol/ext-apps/app-bridge';
import type {
  CallToolRequest,
  CallToolResult,
  LoggingMessageNotification,
} from '@modelcontextprotocol/sdk/types.js';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { toast } from 'react-toastify';
import { EmbeddedResource } from '../api';
import { useTheme } from '../contexts/ThemeContext';
import { errorMessage } from '../utils/conversionUtils';
import { isProtocolSafe, getProtocol } from '../utils/urlSecurity';
import { defineMessages, useIntl } from '../i18n';

const i18n = defineMessages({
  toastTitle: {
    id: 'mcpUIResourceRenderer.toastTitle',
    defaultMessage: 'MCP-UI {messageType} message',
  },
  toastMessageReceived: {
    id: 'mcpUIResourceRenderer.toastMessageReceived',
    defaultMessage: 'Message received for {message}.',
  },
  toastUnsupported: {
    id: 'mcpUIResourceRenderer.toastUnsupported',
    defaultMessage:
      "Message received for {message}. {messageType} messages aren't supported yet, refer to console for more details.",
  },
  openExternalLinkTitle: {
    id: 'mcpUIResourceRenderer.openExternalLinkTitle',
    defaultMessage: 'Open External Link',
  },
  openProtocolLink: {
    id: 'mcpUIResourceRenderer.openProtocolLink',
    defaultMessage: 'Open {protocol} link?',
  },
  openLinkDetail: {
    id: 'mcpUIResourceRenderer.openLinkDetail',
    defaultMessage: 'This will open: {url}',
  },
  cancelButton: {
    id: 'mcpUIResourceRenderer.cancelButton',
    defaultMessage: 'Cancel',
  },
  openButton: {
    id: 'mcpUIResourceRenderer.openButton',
    defaultMessage: 'Open',
  },
});

interface MCPUIResourceRendererProps {
  content: EmbeddedResource & { type: 'resource' };
  appendPromptToChat?: (value: string) => void;
}

// toast component
const ToastComponent = ({
  messageType,
  message,
  isImplemented = true,
}: {
  messageType: string;
  message?: string;
  isImplemented?: boolean;
}) => {
  const intl = useIntl();
  const title = intl.formatMessage(i18n.toastTitle, { messageType });

  return (
    <div className="flex flex-col gap-0 py-2 pr-4">
      <p className="font-bold">{title}</p>
      {isImplemented ? (
        <p>
          {intl.formatMessage(i18n.toastMessageReceived, {
            message: <span className="font-bold">{message}</span>,
          })}
        </p>
      ) : (
        <p>
          {intl.formatMessage(i18n.toastUnsupported, {
            message: <span className="font-bold">{message}</span>,
            messageType: messageType.charAt(0).toUpperCase() + messageType.slice(1),
          })}
        </p>
      )}
    </div>
  );
};

async function fetchMcpUiProxyUrl(): Promise<URL | null> {
  try {
    const baseUrl = await window.electron.getGoosedHostPort();
    const secretKey = await window.electron.getSecretKey();
    if (!baseUrl || !secretKey) {
      console.error('Failed to get goosed host/port or secret key');
      return null;
    }
    return new URL(`${baseUrl}/mcp-app-proxy?secret=${encodeURIComponent(secretKey)}`);
  } catch (error) {
    console.error('Error fetching MCP-UI Proxy URL:', error);
    return null;
  }
}

// MCP-UI resources are delivered inline as either rawHtml (text/html) or an
// externalUrl (text/uri-list). The v7 AppRenderer renders an HTML string in a
// sandboxed iframe, so external URLs are wrapped in a full-bleed iframe.
function resolveHtml(resource: EmbeddedResource['resource']): string | null {
  if (!('text' in resource) || typeof resource.text !== 'string') {
    return null;
  }
  const { text, mimeType } = resource;
  if (mimeType === 'text/uri-list') {
    const url = text
      .split('\n')
      .map((line) => line.trim())
      .find((line) => line && !line.startsWith('#'));
    if (!url) return null;
    return `<!doctype html><html><head><meta charset="utf-8"><style>html,body{margin:0;height:100%}iframe{border:0;width:100%;height:100%}</style></head><body><iframe src="${url}" sandbox="allow-scripts allow-same-origin allow-forms"></iframe></body></html>`;
  }
  return text;
}

export default function MCPUIResourceRenderer({
  content,
  appendPromptToChat,
}: MCPUIResourceRendererProps) {
  const intl = useIntl();
  const { resolvedTheme } = useTheme();
  const [sandboxUrl, setSandboxUrl] = useState<URL | undefined>(undefined);
  const [iframeHeight, setIframeHeight] = useState<number | undefined>(undefined);

  useEffect(() => {
    fetchMcpUiProxyUrl()
      .then((url) => {
        if (url) setSandboxUrl(url);
      })
      .catch(console.error);
  }, []);

  const html = useMemo(() => resolveHtml(content.resource), [content.resource]);

  const handleOpenLink = useCallback(
    async (
      { url }: McpUiOpenLinkRequest['params'],
      _extra: RequestHandlerExtra
    ): Promise<McpUiOpenLinkResult> => {
      try {
        // Safe protocols open directly, unknown protocols require user confirmation.
        // Dangerous protocols are blocked by main.ts in the open-external handler.
        if (isProtocolSafe(url)) {
          await window.electron.openExternal(url);
          return {};
        }

        const protocol = getProtocol(url);
        if (!protocol) {
          return { isError: true, message: `Invalid URL format: ${url}` };
        }

        const result = await window.electron.showMessageBox({
          type: 'question',
          buttons: [intl.formatMessage(i18n.cancelButton), intl.formatMessage(i18n.openButton)],
          defaultId: 0,
          title: intl.formatMessage(i18n.openExternalLinkTitle),
          message: intl.formatMessage(i18n.openProtocolLink, { protocol }),
          detail: intl.formatMessage(i18n.openLinkDetail, { url }),
        });

        if (result.response !== 1) {
          return { isError: true, message: 'User cancelled' };
        }

        await window.electron.openExternal(url);
        return {};
      } catch (error) {
        console.error('Failed to open URL from MCP-UI resource:', error);
        return { isError: true, message: errorMessage(error) };
      }
    },
    [intl]
  );

  const handleMessage = useCallback(
    async (
      { content: blocks }: McpUiMessageRequest['params'],
      _extra: RequestHandlerExtra
    ): Promise<McpUiMessageResult> => {
      if (!appendPromptToChat) {
        return { isError: true, message: 'Prompt handling is not available in this context' };
      }
      const text = blocks
        .filter((block): block is typeof block & { text: string } => block.type === 'text')
        .map((block) => block.text)
        .join('');
      if (!text) {
        return { isError: true, message: 'Message did not contain any text content' };
      }
      try {
        appendPromptToChat(text);
        window.dispatchEvent(new CustomEvent(AppEvents.SCROLL_CHAT_TO_BOTTOM));
        return {};
      } catch (error) {
        console.error('Failed to send prompt to chat:', error);
        return { isError: true, message: errorMessage(error) };
      }
    },
    [appendPromptToChat]
  );

  const handleCallTool = useCallback(
    async ({ name }: CallToolRequest['params']): Promise<CallToolResult> => {
      toast.info(<ToastComponent messageType="tool" message={name} isImplemented={false} />, {
        theme: resolvedTheme,
      });
      return {
        content: [{ type: 'text', text: 'Tool calls are not yet supported for inline MCP-UI.' }],
        isError: true,
      };
    },
    [resolvedTheme]
  );

  const handleLoggingMessage = useCallback(
    ({ data }: LoggingMessageNotification['params']) => {
      const message = typeof data === 'string' ? data : JSON.stringify(data);
      toast.info(<ToastComponent messageType="notify" message={message} isImplemented={true} />, {
        theme: resolvedTheme,
      });
    },
    [resolvedTheme]
  );

  const handleSizeChanged = useCallback(({ height }: McpUiSizeChangedNotification['params']) => {
    if (height !== undefined && height > 0) {
      setIframeHeight(height);
    }
  }, []);

  const sandbox = useMemo(
    () => (sandboxUrl ? { url: sandboxUrl, permissions: 'allow-scripts allow-same-origin' } : null),
    [sandboxUrl]
  );

  if (!html || !sandbox) return null;

  return (
    <div className="mt-3 p-4 border border-border-primary rounded-lg bg-background-secondary">
      <div
        className="overflow-hidden rounded-sm"
        style={iframeHeight ? { height: iframeHeight } : undefined}
      >
        <AppRenderer
          sandbox={sandbox}
          toolName={content.resource.uri}
          html={html}
          hostContext={{ theme: resolvedTheme }}
          onOpenLink={handleOpenLink}
          onMessage={handleMessage}
          onCallTool={handleCallTool}
          onLoggingMessage={handleLoggingMessage}
          onSizeChanged={handleSizeChanged}
          onError={(error) => console.error('MCP-UI render error:', error)}
        />
      </div>
    </div>
  );
}
