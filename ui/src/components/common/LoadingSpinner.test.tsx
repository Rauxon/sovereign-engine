import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import LoadingSpinner from './LoadingSpinner';

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
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

describe('LoadingSpinner', () => {
  it('renders the default "Loading..." message', () => {
    render(<LoadingSpinner />, { wrapper });
    expect(screen.getByText('Loading...')).toBeTruthy();
  });

  it('renders a custom message', () => {
    render(<LoadingSpinner message="Fetching data..." />, { wrapper });
    expect(screen.getByText('Fetching data...')).toBeTruthy();
  });

  it('renders three dot spans for the animation', () => {
    const { container } = render(<LoadingSpinner />, { wrapper });
    // The outer div contains: <style>, 3 dot <span>s, and 1 message <span>
    const outerDiv = container.firstElementChild as HTMLElement;
    const spans = outerDiv.querySelectorAll('span');
    // 3 dots + 1 message = 4 spans
    expect(spans.length).toBe(4);
  });

  it('injects a style element with the pulse keyframe', () => {
    const { container } = render(<LoadingSpinner />, { wrapper });
    const style = container.querySelector('style');
    expect(style).toBeTruthy();
    expect(style!.textContent).toContain('@keyframes pulse');
  });
});
