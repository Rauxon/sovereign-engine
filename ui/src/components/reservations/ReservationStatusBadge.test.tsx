import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import ReservationStatusBadge from './ReservationStatusBadge';
import type { ReservationStatus } from '../../types';

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

describe('ReservationStatusBadge', () => {
  const statuses: { status: ReservationStatus; label: string }[] = [
    { status: 'pending', label: 'Pending' },
    { status: 'approved', label: 'Approved' },
    { status: 'active', label: 'Active' },
    { status: 'completed', label: 'Completed' },
    { status: 'rejected', label: 'Rejected' },
    { status: 'cancelled', label: 'Cancelled' },
  ];

  for (const { status, label } of statuses) {
    it(`renders "${label}" for status "${status}"`, () => {
      render(<ReservationStatusBadge status={status} />, { wrapper });
      expect(screen.getByText(label)).toBeTruthy();
    });
  }

  it('renders as an inline-block span', () => {
    render(<ReservationStatusBadge status="active" />, { wrapper });
    const badge = screen.getByText('Active');
    expect(badge.tagName).toBe('SPAN');
    expect(badge.style.display).toBe('inline-block');
  });

  it('applies font weight 600', () => {
    render(<ReservationStatusBadge status="pending" />, { wrapper });
    const badge = screen.getByText('Pending');
    expect(badge.style.fontWeight).toBe('600');
  });
});
