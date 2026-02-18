import { describe, it, expect, beforeEach, vi } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import {
  lightColors,
  darkColors,
  tableStyles,
  formStyles,
  useTheme,
  ThemeProvider,
} from './theme';

// jsdom does not implement matchMedia; stub it so ThemeProvider can initialise.
beforeEach(() => {
  Object.defineProperty(window, 'matchMedia', {
    writable: true,
    configurable: true,
    value: vi.fn().mockImplementation((query: string) => ({
      matches: false,
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    })),
  });
});

// ---- tableStyles ----

describe('tableStyles', () => {
  it('returns table, th, td style objects', () => {
    const styles = tableStyles(lightColors);
    expect(styles).toHaveProperty('table');
    expect(styles).toHaveProperty('th');
    expect(styles).toHaveProperty('td');
  });

  it('uses the provided colors for light theme', () => {
    const styles = tableStyles(lightColors);

    expect(styles.table.background).toBe(lightColors.tableBg);
    expect(styles.table.border).toContain(lightColors.cardBorder);
    expect(styles.th.background).toBe(lightColors.tableHeaderBg);
    expect(styles.th.color).toBe(lightColors.tableHeaderText);
    expect(styles.th.borderBottom).toContain(lightColors.cardBorder);
    expect(styles.td.borderBottom).toContain(lightColors.tableRowBorder);
  });

  it('uses the provided colors for dark theme', () => {
    const styles = tableStyles(darkColors);

    expect(styles.table.background).toBe(darkColors.tableBg);
    expect(styles.table.border).toContain(darkColors.cardBorder);
    expect(styles.th.background).toBe(darkColors.tableHeaderBg);
    expect(styles.th.color).toBe(darkColors.tableHeaderText);
    expect(styles.td.borderBottom).toContain(darkColors.tableRowBorder);
  });

  it('sets fixed layout properties', () => {
    const styles = tableStyles(lightColors);

    expect(styles.table.width).toBe('100%');
    expect(styles.table.borderCollapse).toBe('collapse');
    expect(styles.table.borderRadius).toBe(8);
    expect(styles.th.textAlign).toBe('left');
    expect(styles.th.fontWeight).toBe(600);
    expect(styles.th.textTransform).toBe('uppercase');
  });
});

// ---- formStyles ----

describe('formStyles', () => {
  it('returns input and label style objects', () => {
    const styles = formStyles(lightColors);
    expect(styles).toHaveProperty('input');
    expect(styles).toHaveProperty('label');
  });

  it('uses the provided colors for light theme', () => {
    const styles = formStyles(lightColors);

    expect(styles.input.border).toContain(lightColors.inputBorder);
    expect(styles.input.background).toBe(lightColors.inputBg);
    expect(styles.input.color).toBe(lightColors.textPrimary);
    expect(styles.label.color).toBe(lightColors.textSecondary);
  });

  it('uses the provided colors for dark theme', () => {
    const styles = formStyles(darkColors);

    expect(styles.input.border).toContain(darkColors.inputBorder);
    expect(styles.input.background).toBe(darkColors.inputBg);
    expect(styles.input.color).toBe(darkColors.textPrimary);
    expect(styles.label.color).toBe(darkColors.textSecondary);
  });

  it('sets fixed layout properties', () => {
    const styles = formStyles(lightColors);

    expect(styles.input.width).toBe('100%');
    expect(styles.input.boxSizing).toBe('border-box');
    expect(styles.input.borderRadius).toBe(4);
    expect(styles.label.display).toBe('block');
    expect(styles.label.fontWeight).toBe(600);
  });
});

// ---- Color palette parity ----

describe('color palettes', () => {
  it('lightColors and darkColors have identical keys', () => {
    const lightKeys = Object.keys(lightColors).sort();
    const darkKeys = Object.keys(darkColors).sort();
    expect(lightKeys).toEqual(darkKeys);
  });

  it('all color values are non-empty strings', () => {
    for (const [key, value] of Object.entries(lightColors)) {
      expect(value, `lightColors.${key}`).toBeTruthy();
      expect(typeof value, `lightColors.${key}`).toBe('string');
    }
    for (const [key, value] of Object.entries(darkColors)) {
      expect(value, `darkColors.${key}`).toBeTruthy();
      expect(typeof value, `darkColors.${key}`).toBe('string');
    }
  });
});

// ---- useTheme hook ----

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

describe('useTheme', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('throws when used outside ThemeProvider', () => {
    expect(() => {
      renderHook(() => useTheme());
    }).toThrow('useTheme must be used within ThemeProvider');
  });

  it('returns the expected shape', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    expect(result.current).toHaveProperty('colors');
    expect(result.current).toHaveProperty('mode');
    expect(result.current).toHaveProperty('setMode');
    expect(result.current).toHaveProperty('resolvedTheme');
    expect(typeof result.current.setMode).toBe('function');
  });

  it('defaults to system mode', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.mode).toBe('system');
  });

  it('returns light colors when resolved theme is light', () => {
    // jsdom matchMedia defaults to not matching dark, so system resolves to light
    const { result } = renderHook(() => useTheme(), { wrapper });
    expect(result.current.resolvedTheme).toBe('light');
    expect(result.current.colors).toBe(lightColors);
  });

  it('returns dark colors when mode is set to dark', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    act(() => {
      result.current.setMode('dark');
    });

    expect(result.current.mode).toBe('dark');
    expect(result.current.resolvedTheme).toBe('dark');
    expect(result.current.colors).toBe(darkColors);
  });

  it('returns light colors when mode is set to light', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    act(() => {
      result.current.setMode('light');
    });

    expect(result.current.mode).toBe('light');
    expect(result.current.resolvedTheme).toBe('light');
    expect(result.current.colors).toBe(lightColors);
  });
});

// ---- ThemeProvider toggle ----

describe('ThemeProvider', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('toggles from light to dark and back', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    // Start in system (light in jsdom)
    expect(result.current.resolvedTheme).toBe('light');
    expect(result.current.colors).toBe(lightColors);

    // Switch to dark
    act(() => {
      result.current.setMode('dark');
    });
    expect(result.current.resolvedTheme).toBe('dark');
    expect(result.current.colors).toBe(darkColors);

    // Switch back to light
    act(() => {
      result.current.setMode('light');
    });
    expect(result.current.resolvedTheme).toBe('light');
    expect(result.current.colors).toBe(lightColors);
  });

  it('persists mode to localStorage', () => {
    const { result } = renderHook(() => useTheme(), { wrapper });

    act(() => {
      result.current.setMode('dark');
    });

    expect(localStorage.getItem('sovereign-theme')).toBe('dark');
  });

  it('reads persisted mode from localStorage', () => {
    localStorage.setItem('sovereign-theme', 'dark');
    const { result } = renderHook(() => useTheme(), { wrapper });

    expect(result.current.mode).toBe('dark');
    expect(result.current.resolvedTheme).toBe('dark');
    expect(result.current.colors).toBe(darkColors);
  });

  it('falls back to system when localStorage has invalid value', () => {
    localStorage.setItem('sovereign-theme', 'invalid');
    const { result } = renderHook(() => useTheme(), { wrapper });

    expect(result.current.mode).toBe('system');
  });
});
