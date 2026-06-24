export interface SessionFolder {
  id: string;
  name: string;
}

interface SessionFoldersState {
  folders: SessionFolder[];
  // Maps a session id to the folder id it belongs to.
  assignments: Record<string, string>;
  // Folder ids that are currently collapsed in the sidebar.
  collapsed: string[];
}

const STORAGE_KEY = 'goose-session-folders';

const emptyState = (): SessionFoldersState => ({
  folders: [],
  assignments: {},
  collapsed: [],
});

function readState(): SessionFoldersState {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return emptyState();

    const parsed = JSON.parse(stored) as Partial<SessionFoldersState>;
    return {
      folders: Array.isArray(parsed.folders) ? parsed.folders : [],
      assignments:
        parsed.assignments && typeof parsed.assignments === 'object' ? parsed.assignments : {},
      collapsed: Array.isArray(parsed.collapsed) ? parsed.collapsed : [],
    };
  } catch (error) {
    console.error('Error reading session folders:', error);
    return emptyState();
  }
}

function writeState(state: SessionFoldersState): SessionFoldersState {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch (error) {
    console.error('Error saving session folders:', error);
  }
  return state;
}

function generateId(): string {
  return `folder-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

export function getSessionFolders(): SessionFoldersState {
  return readState();
}

export function createFolder(name: string): SessionFoldersState {
  const trimmed = name.trim();
  const state = readState();
  if (!trimmed) return state;

  const folder: SessionFolder = { id: generateId(), name: trimmed };
  return writeState({ ...state, folders: [...state.folders, folder] });
}

export function renameFolder(folderId: string, name: string): SessionFoldersState {
  const trimmed = name.trim();
  const state = readState();
  if (!trimmed) return state;

  return writeState({
    ...state,
    folders: state.folders.map((f) => (f.id === folderId ? { ...f, name: trimmed } : f)),
  });
}

export function deleteFolder(folderId: string): SessionFoldersState {
  const state = readState();
  const assignments = { ...state.assignments };
  for (const sessionId of Object.keys(assignments)) {
    if (assignments[sessionId] === folderId) {
      delete assignments[sessionId];
    }
  }
  return writeState({
    folders: state.folders.filter((f) => f.id !== folderId),
    assignments,
    collapsed: state.collapsed.filter((id) => id !== folderId),
  });
}

export function assignSession(sessionId: string, folderId: string | null): SessionFoldersState {
  const state = readState();
  const assignments = { ...state.assignments };
  if (folderId === null) {
    delete assignments[sessionId];
  } else {
    assignments[sessionId] = folderId;
  }
  return writeState({ ...state, assignments });
}

export function toggleFolderCollapsed(folderId: string): SessionFoldersState {
  const state = readState();
  const collapsed = state.collapsed.includes(folderId)
    ? state.collapsed.filter((id) => id !== folderId)
    : [...state.collapsed, folderId];
  return writeState({ ...state, collapsed });
}
