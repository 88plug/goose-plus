import { describe, it, expect, vi, beforeEach } from 'vitest';
import { act, render, type RenderOptions, screen } from '@testing-library/react';
import ModelsBottomBar from './ModelsBottomBar';
import { IntlTestWrapper } from '../../../../i18n/test-utils';

const renderWithIntl = (ui: React.ReactElement, options?: RenderOptions) =>
  render(ui, { wrapper: IntlTestWrapper, ...options });

const createDropdownRef = (): React.RefObject<HTMLDivElement> =>
  ({ current: document.createElement('div') } as React.RefObject<HTMLDivElement>);

let mockCurrentModel: string | null = 'config-model';
let mockCurrentProvider: string | null = 'config-provider';
const mockGetProviders = vi.fn();
const mockRead = vi.fn();
const mockOnModelChanged = vi.fn();

vi.mock('../../../ModelAndProviderContext', () => ({
  useModelAndProvider: () => ({
    currentModel: mockCurrentModel,
    currentProvider: mockCurrentProvider,
  }),
}));

vi.mock('../../../ConfigContext', () => ({
  useConfig: () => ({
    getProviders: mockGetProviders,
    read: mockRead,
  }),
}));

vi.mock('../ModelsSection', () => ({
  MODEL_LOCK_USER_PREF_KEY: 'GOOSE_MODEL_LOCK_USER_PREF',
  MODEL_LOCK_CHANGED_EVENT: 'model-lock-changed',
}));

vi.mock('../modelInterface', () => ({
  getProviderMetadata: vi.fn().mockResolvedValue({ display_name: 'Config Provider' }),
}));

vi.mock('../predefinedModelsUtils', () => ({
  getModelDisplayName: (model: string) => `Display ${model}`,
}));

vi.mock('../../../bottom_menu/BottomMenuAlertPopover', () => ({
  default: () => null,
}));

vi.mock('../../../ui/dropdown-menu', () => ({
  DropdownMenu: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuTrigger: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuContent: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
  DropdownMenuItem: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

vi.mock('../../localInference/ModelSettingsPanel', () => ({
  ModelSettingsPanel: () => null,
}));

vi.mock('../../../ui/scroll-area', () => ({
  ScrollArea: ({ children }: { children: React.ReactNode }) => <div>{children}</div>,
}));

describe('ModelsBottomBar', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockCurrentModel = 'config-model';
    mockCurrentProvider = 'config-provider';
    mockGetProviders.mockResolvedValue([]);
    mockRead.mockResolvedValue(false);
    Object.defineProperty(window, 'appConfig', {
      writable: true,
      configurable: true,
      value: { get: vi.fn().mockReturnValue(false), getAll: vi.fn() },
    });
  });

  it('shows a loading placeholder while the active session model is still loading', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        onModelChanged={mockOnModelChanged}
        sessionLoaded={false}
      />
    );

    expect(screen.getByTestId('model-loading-state')).toHaveTextContent('Loading model...');
  });

  it('shows the active session model once the session has loaded', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId="session-123"
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        sessionModel="session-model"
        sessionProvider="session-provider"
        onModelChanged={mockOnModelChanged}
        sessionLoaded={true}
      />
    );

    expect(screen.getByText('session-model')).toBeInTheDocument();
    expect(screen.queryByTestId('model-loading-state')).not.toBeInTheDocument();
  });

  it('shows the configured model when there is no active session', async () => {
    renderWithIntl(
      <ModelsBottomBar
        sessionId={null}
        dropdownRef={createDropdownRef()}
        setView={vi.fn()}
        onModelChanged={mockOnModelChanged}
      />
    );

    expect(screen.getByText('config-model')).toBeInTheDocument();
    expect(screen.queryByTestId('model-loading-state')).not.toBeInTheDocument();
  });

  it('renders a non-interactive display and no dropdown when the user preference locks the model', async () => {
    mockRead.mockResolvedValue(true);

    await act(async () => {
      renderWithIntl(
        <ModelsBottomBar
          sessionId={null}
          dropdownRef={createDropdownRef()}
          setView={vi.fn()}
          onModelChanged={mockOnModelChanged}
        />
      );
    });

    expect(screen.getByText('config-model')).toBeInTheDocument();
    // The DropdownMenu tree (mocked to always render its children) is entirely
    // absent in locked mode — its "Change Model" item never mounts.
    expect(screen.queryByText('Change Model')).not.toBeInTheDocument();
  });

  it('falls back to the GOOSE_MODEL_LOCK env default when there is no saved user preference', async () => {
    mockRead.mockResolvedValue(null);
    (window.appConfig.get as ReturnType<typeof vi.fn>).mockImplementation(
      (key: string) => key === 'GOOSE_MODEL_LOCK'
    );

    await act(async () => {
      renderWithIntl(
        <ModelsBottomBar
          sessionId={null}
          dropdownRef={createDropdownRef()}
          setView={vi.fn()}
          onModelChanged={mockOnModelChanged}
        />
      );
    });

    expect(screen.getByText('config-model')).toBeInTheDocument();
    expect(screen.queryByText('Change Model')).not.toBeInTheDocument();
  });

  it('shows the interactive dropdown when the model is not locked', async () => {
    mockRead.mockResolvedValue(false);

    await act(async () => {
      renderWithIntl(
        <ModelsBottomBar
          sessionId={null}
          dropdownRef={createDropdownRef()}
          setView={vi.fn()}
          onModelChanged={mockOnModelChanged}
        />
      );
    });

    expect(screen.getByText('config-model')).toBeInTheDocument();
    expect(screen.getByText('Change Model')).toBeInTheDocument();
  });
});
