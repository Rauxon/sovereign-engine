import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import ErrorAlert from './ErrorAlert';

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

describe('ErrorAlert', () => {
  it('renders the error message', () => {
    render(<ErrorAlert message="Something went wrong" />, { wrapper });
    expect(screen.getByText('Something went wrong')).toBeTruthy();
  });

  it('does not render a Retry button when onRetry is not provided', () => {
    render(<ErrorAlert message="Oops" />, { wrapper });
    expect(screen.queryByText('Retry')).toBeNull();
  });

  it('renders a Retry button when onRetry is provided', () => {
    const onRetry = vi.fn();
    render(<ErrorAlert message="Oops" onRetry={onRetry} />, { wrapper });
    expect(screen.getByText('Retry')).toBeTruthy();
  });

  it('calls onRetry when the Retry button is clicked', () => {
    const onRetry = vi.fn();
    render(<ErrorAlert message="Oops" onRetry={onRetry} />, { wrapper });

    fireEvent.click(screen.getByText('Retry'));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it('renders different messages correctly', () => {
    const { rerender } = render(<ErrorAlert message="Error A" />, { wrapper });
    expect(screen.getByText('Error A')).toBeTruthy();

    rerender(
      createElement(ThemeProvider, null, createElement(ErrorAlert, { message: 'Error B' })),
    );
    expect(screen.getByText('Error B')).toBeTruthy();
  });
});
