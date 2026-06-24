import { beforeEach, describe, expect, it } from 'vitest';
import {
  assignSession,
  createFolder,
  deleteFolder,
  getSessionFolders,
  toggleFolderCollapsed,
} from '../sessionFolders';

describe('sessionFolders', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('starts empty', () => {
    const state = getSessionFolders();
    expect(state.folders).toEqual([]);
    expect(state.assignments).toEqual({});
    expect(state.collapsed).toEqual([]);
  });

  it('creates a folder and persists it', () => {
    const state = createFolder('Work');
    expect(state.folders).toHaveLength(1);
    expect(state.folders[0].name).toBe('Work');
    expect(getSessionFolders().folders[0].name).toBe('Work');
  });

  it('ignores blank folder names', () => {
    const state = createFolder('   ');
    expect(state.folders).toHaveLength(0);
  });

  it('assigns and unassigns a session', () => {
    const folderId = createFolder('Repos').folders[0].id;

    const assigned = assignSession('session-1', folderId);
    expect(assigned.assignments['session-1']).toBe(folderId);

    const cleared = assignSession('session-1', null);
    expect(cleared.assignments['session-1']).toBeUndefined();
  });

  it('removes assignments when a folder is deleted', () => {
    const folderId = createFolder('Temp').folders[0].id;
    assignSession('session-1', folderId);

    const state = deleteFolder(folderId);
    expect(state.folders).toHaveLength(0);
    expect(state.assignments['session-1']).toBeUndefined();
  });

  it('toggles collapsed state', () => {
    const folderId = createFolder('Notes').folders[0].id;

    expect(toggleFolderCollapsed(folderId).collapsed).toContain(folderId);
    expect(toggleFolderCollapsed(folderId).collapsed).not.toContain(folderId);
  });
});
