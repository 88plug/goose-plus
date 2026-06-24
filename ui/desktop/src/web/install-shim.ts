import type Electron from 'electron';
import type { Recipe } from '../recipe';
import type { GooseApp } from '../api';
import type { Settings, SettingKey } from '../utils/settings';
import { defaultSettings } from '../utils/settings';
import { apiHost, secret, version, config } from './runtime';

interface NotificationData {
  title: string;
  body: string;
}

interface MessageBoxOptions {
  type?: 'none' | 'info' | 'error' | 'question' | 'warning';
  buttons?: string[];
  defaultId?: number;
  title?: string;
  message: string;
  detail?: string;
}

interface MessageBoxResponse {
  response: number;
  checkboxChecked?: boolean;
}

interface SaveDialogOptions {
  title?: string;
  defaultPath?: string;
  buttonLabel?: string;
  filters?: Array<{ name: string; extensions: string[] }>;
  message?: string;
  nameFieldLabel?: string;
  showsTagField?: boolean;
}

interface SaveDialogResponse {
  canceled: boolean;
  filePath?: string;
}

interface FileResponse {
  file: string;
  filePath: string;
  error: string | null;
  found: boolean;
}

interface UpdaterEvent {
  event: string;
  data?: unknown;
}

interface CreateChatWindowOptions {
  query?: string;
  dir?: string;
  version?: string;
  resumeSessionId?: string;
  viewType?: string;
  recipeId?: string;
}

const localStorageKeyMap: Partial<Record<SettingKey, string>> = {
  theme: 'theme',
  useSystemTheme: 'use_system_theme',
  responseStyle: 'response_style',
  showPricing: 'show_pricing',
  sessionSharing: 'session_sharing_config',
  seenAnnouncementIds: 'seenAnnouncementIds',
};

const settingsStorageKey = 'goose_web_settings';

function parseLocalStorageValue<K extends SettingKey>(
  key: K,
  rawValue: string
): Settings[K] | null {
  try {
    switch (key) {
      case 'theme':
        return (rawValue === 'dark' || rawValue === 'light' ? rawValue : null) as Settings[K];
      case 'useSystemTheme':
        return (rawValue === 'true') as unknown as Settings[K];
      case 'responseStyle':
        return rawValue as Settings[K];
      case 'showPricing':
        return (rawValue === 'true') as unknown as Settings[K];
      case 'sessionSharing':
        return JSON.parse(rawValue) as Settings[K];
      case 'seenAnnouncementIds':
        return JSON.parse(rawValue) as Settings[K];
      default:
        return null;
    }
  } catch {
    return null;
  }
}

function readSettingsStore(): Partial<Settings> {
  try {
    const raw = localStorage.getItem(settingsStorageKey);
    if (raw === null) {
      return {};
    }
    return JSON.parse(raw) as Partial<Settings>;
  } catch {
    return {};
  }
}

function writeSettingsStore(store: Partial<Settings>): void {
  try {
    localStorage.setItem(settingsStorageKey, JSON.stringify(store));
  } catch (error) {
    console.error('Failed to persist setting', error);
  }
}

function detectPlatform(): string {
  const platform = navigator.platform.toLowerCase();
  const ua = navigator.userAgent.toLowerCase();
  if (platform.includes('mac') || ua.includes('mac')) {
    return 'darwin';
  }
  if (platform.includes('win') || ua.includes('win')) {
    return 'win32';
  }
  return 'linux';
}

function openExternalUrl(url: string): void {
  window.open(url, '_blank', 'noopener');
}

type EventCallback = (event: Electron.IpcRendererEvent, ...args: unknown[]) => void;
type BusListener = (event: Event) => void;

const eventBus = window;
const listenerMap = new WeakMap<EventCallback, BusListener>();

function makeRendererEvent(): Electron.IpcRendererEvent {
  return {
    sender: {} as Electron.IpcRenderer,
    senderId: 0,
    ports: [],
  } as unknown as Electron.IpcRendererEvent;
}

const mouseBackCallbacks = new WeakMap<() => void, BusListener>();

const electronAPI: (typeof window)['electron'] = {
  platform: detectPlatform(),
  arch: 'x64',
  reactReady: () => {},
  getConfig: () => config,
  hideWindow: () => {
    window.close();
  },
  directoryChooser: () =>
    Promise.resolve({ canceled: true, filePaths: [] } as Electron.OpenDialogReturnValue),
  createChatWindow: (_options?: CreateChatWindowOptions) => {},
  logInfo: (txt: string) => console.log(txt),
  showNotification: (data: NotificationData) => {
    const Notif = window.Notification;
    if (typeof Notif === 'undefined') {
      return;
    }
    const show = () => new Notif(data.title, { body: data.body });
    if (Notif.permission === 'granted') {
      show();
    } else if (Notif.permission !== 'denied') {
      Notif.requestPermission()
        .then((permission) => {
          if (permission === 'granted') {
            show();
          }
        })
        .catch(() => {});
    }
  },
  showMessageBox: (options: MessageBoxOptions): Promise<MessageBoxResponse> => {
    const buttons = options.buttons ?? ['OK'];
    const text = options.detail ? `${options.message}\n\n${options.detail}` : options.message;
    if (buttons.length > 1) {
      const confirmed = window.confirm(text);
      const yesIndex = options.defaultId ?? 0;
      const noIndex = buttons.length > 1 ? (yesIndex === 0 ? 1 : 0) : 0;
      return Promise.resolve({ response: confirmed ? yesIndex : noIndex, checkboxChecked: false });
    }
    window.alert(text);
    return Promise.resolve({ response: 0, checkboxChecked: false });
  },
  showSaveDialog: (_options: SaveDialogOptions): Promise<SaveDialogResponse> =>
    Promise.resolve({ canceled: true }),
  openInChrome: (url: string) => openExternalUrl(url),
  reloadApp: () => window.location.reload(),
  checkForOllama: () => Promise.resolve(false),
  checkMesh: () => Promise.resolve({ running: false, installed: false, models: [] }),
  startMesh: (_args: string[]) =>
    Promise.resolve({ started: false, error: 'not supported in web' }),
  stopMesh: () => Promise.resolve({ stopped: false }),
  selectFileOrDirectory: (_defaultPath?: string): Promise<string | null> =>
    new Promise((resolve) => {
      const input = document.createElement('input');
      input.type = 'file';
      input.onchange = () => {
        const file = input.files?.[0];
        resolve(file ? file.name : null);
      };
      input.oncancel = () => resolve(null);
      input.click();
    }),
  selectImportSessionFile: (): Promise<{
    filePath: string;
    contents: string;
    error?: string;
  } | null> =>
    new Promise((resolve) => {
      const input = document.createElement('input');
      input.type = 'file';
      input.onchange = () => {
        const file = input.files?.[0];
        if (!file) {
          resolve(null);
          return;
        }
        const reader = new FileReader();
        reader.onload = () =>
          resolve({ filePath: file.name, contents: String(reader.result ?? '') });
        reader.onerror = () =>
          resolve({ filePath: file.name, contents: '', error: 'failed to read file' });
        reader.readAsText(file);
      };
      input.oncancel = () => resolve(null);
      input.click();
    }),
  getBinaryPath: (_binaryName: string): Promise<string> => Promise.resolve(''),
  readFile: (directory: string): Promise<FileResponse> =>
    Promise.resolve({ file: '', filePath: directory, error: 'not supported in web', found: false }),
  writeFile: (_directory: string, _content: string): Promise<boolean> => Promise.resolve(false),
  ensureDirectory: (_dirPath: string): Promise<boolean> => Promise.resolve(false),
  listFiles: (_dirPath: string, _extension?: string): Promise<string[]> => Promise.resolve([]),
  getAllowedExtensions: (): Promise<string[]> => Promise.resolve([]),
  getPathForFile: (file: File): string => file.name,
  setMenuBarIcon: (_show: boolean): Promise<boolean> => Promise.resolve(false),
  getMenuBarIconState: (): Promise<boolean> => Promise.resolve(false),
  setDockIcon: (_show: boolean): Promise<boolean> => Promise.resolve(false),
  getDockIconState: (): Promise<boolean> => Promise.resolve(false),
  getSetting: <K extends SettingKey>(key: K): Promise<Settings[K]> => {
    try {
      const localStorageKey = localStorageKeyMap[key];
      if (localStorageKey) {
        const rawValue = localStorage.getItem(localStorageKey);
        if (rawValue !== null) {
          const parsed = parseLocalStorageValue(key, rawValue);
          if (parsed !== null) {
            return Promise.resolve(parsed);
          }
        }
      }
      const store = readSettingsStore();
      const stored = store[key];
      if (stored !== undefined) {
        return Promise.resolve(stored as Settings[K]);
      }
      return Promise.resolve(defaultSettings[key]);
    } catch (error) {
      console.error(`Failed to get setting '${key}', using default`, error);
      return Promise.resolve(defaultSettings[key]);
    }
  },
  setSetting: <K extends SettingKey>(key: K, value: Settings[K]): Promise<void> => {
    const localStorageKey = localStorageKeyMap[key];
    if (localStorageKey) {
      localStorage.removeItem(localStorageKey);
    }
    const store = readSettingsStore();
    store[key] = value;
    writeSettingsStore(store);
    return Promise.resolve();
  },
  getSecretKey: (): Promise<string> => Promise.resolve(secret),
  getGoosedHostPort: (): Promise<string | null> => Promise.resolve(apiHost),
  getAcpUrl: (): Promise<string | null> => Promise.resolve(null),
  setWakelock: (_enable: boolean): Promise<boolean> => Promise.resolve(false),
  getWakelockState: (): Promise<boolean> => Promise.resolve(false),
  setSpellcheck: (_enable: boolean): Promise<boolean> => Promise.resolve(false),
  getSpellcheckState: (): Promise<boolean> => Promise.resolve(false),
  openNotificationsSettings: (): Promise<boolean> => Promise.resolve(false),
  isAnyWindowFocused: (): Promise<boolean> => Promise.resolve(document.hasFocus()),
  getIsFullScreen: (): Promise<boolean> => Promise.resolve(!!document.fullscreenElement),
  onMouseBackButtonClicked: (callback: () => void) => {
    const listener: BusListener = () => callback();
    mouseBackCallbacks.set(callback, listener);
    eventBus.addEventListener('mouse-back-button-clicked', listener);
  },
  offMouseBackButtonClicked: (callback: () => void) => {
    const listener = mouseBackCallbacks.get(callback);
    if (listener) {
      eventBus.removeEventListener('mouse-back-button-clicked', listener);
      mouseBackCallbacks.delete(callback);
    }
  },
  on: (channel: string, callback: EventCallback) => {
    const listener: BusListener = (event) => {
      const detail = (event as CustomEvent<unknown[]>).detail ?? [];
      callback(makeRendererEvent(), ...detail);
    };
    listenerMap.set(callback, listener);
    eventBus.addEventListener(channel, listener);
  },
  off: (channel: string, callback: EventCallback) => {
    const listener = listenerMap.get(callback);
    if (listener) {
      eventBus.removeEventListener(channel, listener);
      listenerMap.delete(callback);
    }
  },
  emit: (channel: string, ...args: unknown[]) => {
    eventBus.dispatchEvent(new CustomEvent(channel, { detail: args }));
  },
  broadcastThemeChange: (_themeData: {
    mode: string;
    useSystemTheme: boolean;
    theme: string;
    tokensUpdated?: boolean;
  }) => {},
  openExternal: (url: string): Promise<void> => {
    openExternalUrl(url);
    return Promise.resolve();
  },
  getVersion: (): string => version,
  checkForUpdates: (): Promise<{ updateInfo: unknown; error: string | null }> =>
    Promise.resolve({ updateInfo: null, error: null }),
  downloadUpdate: (): Promise<{ success: boolean; error: string | null }> =>
    Promise.resolve({ success: false, error: 'not supported in web' }),
  installUpdate: (): void => {},
  restartApp: (): void => {
    window.location.reload();
  },
  onUpdaterEvent: (_callback: (event: UpdaterEvent) => void): void => {},
  getUpdateState: (): Promise<{ updateAvailable: boolean; latestVersion?: string } | null> =>
    Promise.resolve(null),
  isUsingGitHubFallback: (): Promise<boolean> => Promise.resolve(false),
  getAutoDownloadDisabled: (): Promise<boolean> => Promise.resolve(true),
  closeWindow: () => {
    window.close();
  },
  hasAcceptedRecipeBefore: (recipe: Recipe): Promise<boolean> => {
    try {
      const accepted = JSON.parse(
        localStorage.getItem('goose_web_accepted_recipes') ?? '[]'
      ) as string[];
      return Promise.resolve(accepted.includes(JSON.stringify(recipe)));
    } catch {
      return Promise.resolve(false);
    }
  },
  recordRecipeHash: (recipe: Recipe): Promise<boolean> => {
    try {
      const accepted = JSON.parse(
        localStorage.getItem('goose_web_accepted_recipes') ?? '[]'
      ) as string[];
      const hash = JSON.stringify(recipe);
      if (!accepted.includes(hash)) {
        accepted.push(hash);
        localStorage.setItem('goose_web_accepted_recipes', JSON.stringify(accepted));
      }
      return Promise.resolve(true);
    } catch {
      return Promise.resolve(false);
    }
  },
  openDirectoryInExplorer: (_directoryPath: string): Promise<boolean> => Promise.resolve(false),
  launchApp: (_app: GooseApp): Promise<void> => Promise.resolve(),
  refreshApp: (_app: GooseApp): Promise<void> => Promise.resolve(),
  closeApp: (_appName: string): Promise<void> => Promise.resolve(),
  addRecentDir: (_dir: string): Promise<boolean> => Promise.resolve(false),
  listRecentDirs: (): Promise<string[]> => Promise.resolve([]),
  listGitWorktreeDirs: (_dir: string): Promise<string[]> => Promise.resolve([]),
};

const appConfigAPI: (typeof window)['appConfig'] = {
  get: (key: string) => config[key],
  getAll: () => config,
};

window.electron = electronAPI;
window.appConfig = appConfigAPI;
