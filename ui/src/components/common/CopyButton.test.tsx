import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, act, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import CopyButton from './CopyButton';

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

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

  Object.defineProperty(navigator, 'clipboard', {
    writable: true,
    configurable: true,
    value: {
      writeText: vi.fn().mockResolvedValue(undefined),
    },
  });

  vi.useFakeTimers();
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

describe('CopyButton', () => {
  it('renders the default "Copy" label', () => {
    render(<CopyButton text="hello" />, { wrapper });
    expect(screen.getByText('Copy')).toBeTruthy();
  });

  it('renders a custom label', () => {
    render(<CopyButton text="hello" label="Copy Token" />, { wrapper });
    expect(screen.getByText('Copy Token')).toBeTruthy();
  });

  it('copies text to clipboard on click and shows "Copied!"', async () => {
    render(<CopyButton text="secret-value" />, { wrapper });

    await act(async () => {
      fireEvent.click(screen.getByText('Copy'));
    });

    expect(navigator.clipboard.writeText).toHaveBeenCalledWith('secret-value');
    expect(screen.getByText('Copied!')).toBeTruthy();
  });

  it('reverts label back to original after 2 seconds', async () => {
    render(<CopyButton text="data" label="Copy It" />, { wrapper });

    await act(async () => {
      fireEvent.click(screen.getByText('Copy It'));
    });

    expect(screen.getByText('Copied!')).toBeTruthy();

    act(() => {
      vi.advanceTimersByTime(2000);
    });

    expect(screen.getByText('Copy It')).toBeTruthy();
  });

  it('does not show "Copied!" when clipboard API fails', async () => {
    (navigator.clipboard.writeText as ReturnType<typeof vi.fn>).mockRejectedValueOnce(
      new Error('not allowed'),
    );

    render(<CopyButton text="data" />, { wrapper });

    await act(async () => {
      fireEvent.click(screen.getByText('Copy'));
    });

    // Label should remain "Copy" since the catch returns early
    expect(screen.getByText('Copy')).toBeTruthy();
    expect(screen.queryByText('Copied!')).toBeNull();
  });
});
