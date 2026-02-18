import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import ThemeToggle from './ThemeToggle';

afterEach(cleanup);

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
  localStorage.clear();
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

describe('ThemeToggle', () => {
  it('renders a button', () => {
    render(<ThemeToggle />, { wrapper });
    expect(screen.getByRole('button')).toBeTruthy();
  });

  it('starts with system theme and shows the system icon', () => {
    render(<ThemeToggle />, { wrapper });
    const button = screen.getByRole('button');
    expect(button.getAttribute('title')).toBe('Theme: System');
    // System icon is the monitor emoji
    expect(button.textContent).toBe('\u{1F5A5}');
  });

  it('cycles system -> light on first click', () => {
    render(<ThemeToggle />, { wrapper });
    const button = screen.getByRole('button');

    fireEvent.click(button);

    expect(button.getAttribute('title')).toBe('Theme: Light');
    expect(button.textContent).toBe('\u2600');
  });

  it('cycles system -> light -> dark on two clicks', () => {
    render(<ThemeToggle />, { wrapper });
    const button = screen.getByRole('button');

    fireEvent.click(button); // system -> light
    fireEvent.click(button); // light -> dark

    expect(button.getAttribute('title')).toBe('Theme: Dark');
    expect(button.textContent).toBe('\u{1F319}');
  });

  it('cycles back to system after three clicks', () => {
    render(<ThemeToggle />, { wrapper });
    const button = screen.getByRole('button');

    fireEvent.click(button); // system -> light
    fireEvent.click(button); // light -> dark
    fireEvent.click(button); // dark -> system

    expect(button.getAttribute('title')).toBe('Theme: System');
    expect(button.textContent).toBe('\u{1F5A5}');
  });

  it('persists mode changes to localStorage', () => {
    render(<ThemeToggle />, { wrapper });
    const button = screen.getByRole('button');

    fireEvent.click(button); // system -> light
    expect(localStorage.getItem('sovereign-theme')).toBe('light');

    fireEvent.click(button); // light -> dark
    expect(localStorage.getItem('sovereign-theme')).toBe('dark');
  });
});
