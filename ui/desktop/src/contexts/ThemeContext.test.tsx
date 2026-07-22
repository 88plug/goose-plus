import React from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import { ThemeProvider, useTheme } from './ThemeContext';

function mockMatchMedia(prefersDark: boolean) {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: prefersDark,
      media: query,
      onchange: null,
      addListener: vi.fn(),
      removeListener: vi.fn(),
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
}

function ResolvedThemeProbe() {
  const { resolvedTheme } = useTheme();
  return <div data-testid="resolved-theme">{resolvedTheme}</div>;
}

function PreferenceProbe() {
  const { userThemePreference } = useTheme();
  return <div data-testid="preference">{userThemePreference}</div>;
}

describe('ThemeProvider', () => {
  beforeEach(() => {
    (window as any).electron.getSetting = vi.fn((key: string) => {
      if (key === 'useSystemTheme') return Promise.resolve(true);
      if (key === 'theme') return Promise.resolve('light');
      return Promise.resolve(undefined);
    });
  });

  it('resolves to the OS dark theme immediately, before settings load', () => {
    mockMatchMedia(true);

    render(
      <ThemeProvider>
        <ResolvedThemeProbe />
      </ThemeProvider>
    );

    expect(screen.getByTestId('resolved-theme').textContent).toBe('dark');
  });

  it('resolves to the OS light theme immediately, before settings load', () => {
    mockMatchMedia(false);

    render(
      <ThemeProvider>
        <ResolvedThemeProbe />
      </ThemeProvider>
    );

    expect(screen.getByTestId('resolved-theme').textContent).toBe('light');
  });

  it('still settles on the saved preference once settings load', async () => {
    mockMatchMedia(true);
    (window as any).electron.getSetting = vi.fn((key: string) => {
      if (key === 'useSystemTheme') return Promise.resolve(false);
      if (key === 'theme') return Promise.resolve('light');
      return Promise.resolve(undefined);
    });

    render(
      <ThemeProvider>
        <ResolvedThemeProbe />
      </ThemeProvider>
    );

    await waitFor(() => {
      expect(screen.getByTestId('resolved-theme').textContent).toBe('light');
    });
  });

  it('seeds the theme preference to system so it matches the resolved OS theme before settings load', () => {
    mockMatchMedia(true);

    render(
      <ThemeProvider>
        <PreferenceProbe />
      </ThemeProvider>
    );

    expect(screen.getByTestId('preference').textContent).toBe('system');
  });

  it('mounts without crashing when window.matchMedia is unavailable', () => {
    (window as any).matchMedia = undefined;

    render(
      <ThemeProvider>
        <ResolvedThemeProbe />
      </ThemeProvider>
    );

    expect(screen.getByTestId('resolved-theme').textContent).toBe('light');
  });
});
