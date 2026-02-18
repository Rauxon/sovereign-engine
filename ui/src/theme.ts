import { createContext, useContext, useState, useEffect, useMemo, useCallback, createElement } from 'react';
import type { ReactNode } from 'react';

// ---- Types ----

export type ThemeMode = 'system' | 'light' | 'dark';
export type ResolvedTheme = 'light' | 'dark';

export interface ThemeColors {
  pageBg: string;
  cardBg: string;
  cardBorder: string;
  textPrimary: string;
  textSecondary: string;
  textMuted: string;

  navBg: string;
  navText: string;
  navTextActive: string;
  navTextInactive: string;
  navSeparator: string;

  inputBg: string;
  inputBorder: string;

  buttonPrimary: string;
  buttonPrimaryDisabled: string;
  buttonDanger: string;
  buttonDisabled: string;

  tableBg: string;
  tableHeaderBg: string;
  tableHeaderText: string;
  tableRowBorder: string;

  badgeSuccessBg: string;
  badgeSuccessText: string;
  badgeDangerBg: string;
  badgeDangerText: string;
  badgeWarningBg: string;
  badgeWarningText: string;
  badgeInfoBg: string;
  badgeInfoText: string;
  badgeNeutralBg: string;
  badgeNeutralText: string;
  badgePurpleBg: string;
  badgePurpleText: string;

  successText: string;
  dangerText: string;
  warningText: string;

  overlayBg: string;
  dialogBg: string;
  dialogShadow: string;

  spinnerColor: string;
  chartGrid: string;
  link: string;

  // Specific semantic tokens
  warningBannerBg: string;
  warningBannerBorder: string;
  warningBannerText: string;

  pickerBg: string;
  pickerBorder: string;

  progressBarBg: string;
}

// ---- Color Palettes ----

export const lightColors: ThemeColors = {
  pageBg: '#f9fafb',
  cardBg: '#fff',
  cardBorder: '#e5e7eb',
  textPrimary: '#1a1a2e',
  textSecondary: '#333',
  textMuted: '#666',

  navBg: '#1a1a2e',
  navText: '#eee',
  navTextActive: '#fff',
  navTextInactive: '#aaa',
  navSeparator: '#444',

  inputBg: '#fff',
  inputBorder: '#d1d5db',

  buttonPrimary: '#2563eb',
  buttonPrimaryDisabled: '#93c5fd',
  buttonDanger: '#dc2626',
  buttonDisabled: '#e5e7eb',

  tableBg: '#fff',
  tableHeaderBg: '#f9fafb',
  tableHeaderText: '#555',
  tableRowBorder: '#f3f4f6',

  badgeSuccessBg: '#ecfdf5',
  badgeSuccessText: '#16a34a',
  badgeDangerBg: '#fef2f2',
  badgeDangerText: '#dc2626',
  badgeWarningBg: '#fef3c7',
  badgeWarningText: '#92400e',
  badgeInfoBg: '#dbeafe',
  badgeInfoText: '#1e40af',
  badgeNeutralBg: '#f3f4f6',
  badgeNeutralText: '#888',
  badgePurpleBg: '#ede9fe',
  badgePurpleText: '#7c3aed',

  successText: '#16a34a',
  dangerText: '#dc2626',
  warningText: '#f59e0b',

  overlayBg: 'rgba(0,0,0,0.5)',
  dialogBg: '#fff',
  dialogShadow: '0 4px 24px rgba(0,0,0,0.2)',

  spinnerColor: '#888',
  chartGrid: '#e5e7eb',
  link: '#2563eb',

  warningBannerBg: '#fefce8',
  warningBannerBorder: '#facc15',
  warningBannerText: '#92400e',

  pickerBg: '#f0f5ff',
  pickerBorder: '#2563eb',

  progressBarBg: '#e5e7eb',
};

export const darkColors: ThemeColors = {
  pageBg: '#0f1117',
  cardBg: '#1a1d27',
  cardBorder: '#2d3141',
  textPrimary: '#e5e7eb',
  textSecondary: '#c9cdd5',
  textMuted: '#9ca3af',

  navBg: '#0d0f15',
  navText: '#e5e7eb',
  navTextActive: '#fff',
  navTextInactive: '#9ca3af',
  navSeparator: '#374151',

  inputBg: '#1f2231',
  inputBorder: '#374151',

  buttonPrimary: '#3b82f6',
  buttonPrimaryDisabled: '#1e3a5f',
  buttonDanger: '#ef4444',
  buttonDisabled: '#374151',

  tableBg: '#1a1d27',
  tableHeaderBg: '#151822',
  tableHeaderText: '#9ca3af',
  tableRowBorder: '#2d3141',

  badgeSuccessBg: '#052e16',
  badgeSuccessText: '#4ade80',
  badgeDangerBg: '#450a0a',
  badgeDangerText: '#f87171',
  badgeWarningBg: '#451a03',
  badgeWarningText: '#fbbf24',
  badgeInfoBg: '#172554',
  badgeInfoText: '#60a5fa',
  badgeNeutralBg: '#1f2937',
  badgeNeutralText: '#9ca3af',
  badgePurpleBg: '#2e1065',
  badgePurpleText: '#a78bfa',

  successText: '#4ade80',
  dangerText: '#f87171',
  warningText: '#fbbf24',

  overlayBg: 'rgba(0,0,0,0.7)',
  dialogBg: '#1a1d27',
  dialogShadow: '0 4px 24px rgba(0,0,0,0.5)',

  spinnerColor: '#9ca3af',
  chartGrid: '#2d3141',
  link: '#60a5fa',

  warningBannerBg: '#451a03',
  warningBannerBorder: '#854d0e',
  warningBannerText: '#fbbf24',

  pickerBg: '#172554',
  pickerBorder: '#3b82f6',

  progressBarBg: '#2d3141',
};

// ---- Context ----

interface ThemeContextValue {
  colors: ThemeColors;
  mode: ThemeMode;
  setMode: (mode: ThemeMode) => void;
  resolvedTheme: ResolvedTheme;
}

const ThemeContext = createContext<ThemeContextValue | null>(null);

const STORAGE_KEY = 'sovereign-theme';

function getSystemTheme(): ResolvedTheme {
  if (globalThis.window === undefined) return 'light';
  return globalThis.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
}

// ---- Provider ----

export function ThemeProvider({ children }: Readonly<{ children: ReactNode }>) {
  const [mode, setMode] = useState<ThemeMode>(() => {
    if (globalThis.window === undefined) return 'system';
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === 'light' || stored === 'dark' || stored === 'system') return stored;
    return 'system';
  });

  const [systemTheme, setSystemTheme] = useState<ResolvedTheme>(getSystemTheme);

  // Listen for OS theme changes
  useEffect(() => {
    const mq = globalThis.matchMedia('(prefers-color-scheme: dark)');
    const handler = (e: MediaQueryListEvent) => {
      setSystemTheme(e.matches ? 'dark' : 'light');
    };
    mq.addEventListener('change', handler);
    return () => mq.removeEventListener('change', handler);
  }, []);

  const handleSetMode = useCallback((newMode: ThemeMode) => {
    setMode(newMode);
    localStorage.setItem(STORAGE_KEY, newMode);
  }, []);

  const resolvedTheme: ResolvedTheme = mode === 'system' ? systemTheme : mode;
  const colors = resolvedTheme === 'dark' ? darkColors : lightColors;

  // Apply to document
  useEffect(() => {
    document.documentElement.dataset.theme = resolvedTheme;
    document.body.style.background = colors.pageBg;
    document.body.style.color = colors.textPrimary;
  }, [resolvedTheme, colors]);

  const value = useMemo(
    () => ({ colors, mode, setMode: handleSetMode, resolvedTheme }),
    [colors, mode, handleSetMode, resolvedTheme],
  );

  return createElement(ThemeContext.Provider, { value }, children);
}

// ---- Hook ----

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used within ThemeProvider');
  return ctx;
}
