import { render, type RenderOptions, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { providers as fetchProviders, ProviderDetails } from '../../api';
import { IntlTestWrapper } from '../../i18n/test-utils';
import ProviderSelector from './ProviderSelector';

vi.mock('../../api', () => ({
  providers: vi.fn(),
  createCustomProvider: vi.fn(),
}));

vi.mock('../../contexts/FeaturesContext', () => ({
  useFeatures: () => ({ localInference: false }),
}));

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const fetchProvidersMock = vi.mocked(fetchProviders);

function makeProvider(overrides: Partial<ProviderDetails>): ProviderDetails {
  return {
    name: 'anthropic',
    is_configured: false,
    provider_type: 'Preferred',
    metadata: {
      name: 'anthropic',
      display_name: 'Anthropic',
      description: '',
      default_model: 'claude-sonnet-5',
      known_models: [],
      model_doc_link: '',
      config_keys: [{ name: 'ANTHROPIC_API_KEY', secret: true, required: true, oauth_flow: false }],
    },
    ...overrides,
  } as ProviderDetails;
}

describe('ProviderSelector quick setup', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('shows a quick-setup card when a provider is already configured via env', async () => {
    fetchProvidersMock.mockResolvedValueOnce({
      data: [makeProvider({ is_configured: true })],
    } as Awaited<ReturnType<typeof fetchProviders>>);

    const onConfigured = vi.fn();
    renderWithIntl(<ProviderSelector onConfigured={onConfigured} />);

    expect(await screen.findByText('Quick Setup with Anthropic')).toBeInTheDocument();

    await userEvent.click(screen.getByText('Quick Setup with Anthropic'));
    expect(onConfigured).toHaveBeenCalledWith('anthropic', 'claude-sonnet-5');
  });

  it('does not show a quick-setup card when nothing is detected', async () => {
    fetchProvidersMock.mockResolvedValueOnce({
      data: [makeProvider({ is_configured: false })],
    } as Awaited<ReturnType<typeof fetchProviders>>);

    renderWithIntl(<ProviderSelector onConfigured={vi.fn()} />);

    await screen.findByText('Connect to a Provider');
    expect(screen.queryByText(/Quick Setup with/)).not.toBeInTheDocument();
  });

  it('ignores the always-configured local provider', async () => {
    fetchProvidersMock.mockResolvedValueOnce({
      data: [
        makeProvider({
          name: 'local',
          is_configured: true,
          metadata: {
            name: 'local',
            display_name: 'Local',
            description: '',
            default_model: 'local-model',
            known_models: [],
            model_doc_link: '',
            config_keys: [],
          },
        }),
      ],
    } as Awaited<ReturnType<typeof fetchProviders>>);

    renderWithIntl(<ProviderSelector onConfigured={vi.fn()} />);

    await screen.findByText('Connect to a Provider');
    expect(screen.queryByText(/Quick Setup with/)).not.toBeInTheDocument();
  });
});
