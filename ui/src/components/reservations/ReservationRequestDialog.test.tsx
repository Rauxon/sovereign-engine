import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import ReservationRequestDialog from './ReservationRequestDialog';
import { createReservation } from '../../api';

vi.mock('../../api', () => ({
  createReservation: vi.fn(),
}));

const mockedCreateReservation = vi.mocked(createReservation);

afterEach(cleanup);

beforeEach(() => {
  vi.clearAllMocks();

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

  HTMLDialogElement.prototype.showModal = vi.fn();
  HTMLDialogElement.prototype.close = vi.fn();
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

function renderDialog(props: Partial<React.ComponentProps<typeof ReservationRequestDialog>> = {}) {
  const defaults = {
    startTime: '2026-02-15T09:00:00',
    endTime: '2026-02-15T17:00:00',
    onCreated: vi.fn(),
    onCancel: vi.fn(),
  };
  const merged = { ...defaults, ...props };
  if (!props.onCreated) merged.onCreated = vi.fn();
  if (!props.onCancel) merged.onCancel = vi.fn();
  const result = render(<ReservationRequestDialog {...merged} />, { wrapper });
  return { ...result, ...merged };
}

describe('ReservationRequestDialog', () => {
  it('calls showModal on mount', () => {
    renderDialog();
    expect(HTMLDialogElement.prototype.showModal).toHaveBeenCalled();
  });

  it('renders the dialog title', () => {
    renderDialog();
    expect(screen.getByText('Request Reservation')).toBeTruthy();
  });

  it('renders start and end time inputs pre-filled', () => {
    renderDialog({
      startTime: '2026-03-10T14:00:00',
      endTime: '2026-03-10T18:00:00',
    });

    const startInput = screen.getByLabelText('Start') as HTMLInputElement;
    const endInput = screen.getByLabelText('End') as HTMLInputElement;

    // toDatetimeLocal slices to first 16 chars
    expect(startInput.value).toBe('2026-03-10T14:00');
    expect(endInput.value).toBe('2026-03-10T18:00');
  });

  it('renders a reason textarea', () => {
    renderDialog();
    expect(screen.getByLabelText('Reason (optional)')).toBeTruthy();
  });

  it('calls createReservation on submit', async () => {
    mockedCreateReservation.mockResolvedValue({ id: 'res-1', status: 'pending' });
    const onCreated = vi.fn();

    renderDialog({ onCreated });

    fireEvent.click(screen.getByText('Request Booking'));

    await waitFor(() => {
      expect(mockedCreateReservation).toHaveBeenCalledOnce();
    });

    // The call should have converted local times to UTC ISO strings
    const callArgs = mockedCreateReservation.mock.calls[0][0];
    expect(callArgs.start_time).toBeDefined();
    expect(callArgs.end_time).toBeDefined();

    await waitFor(() => {
      expect(onCreated).toHaveBeenCalledOnce();
    });
  });

  it('passes reason to createReservation when provided', async () => {
    mockedCreateReservation.mockResolvedValue({ id: 'res-1', status: 'pending' });

    renderDialog();

    fireEvent.change(screen.getByLabelText('Reason (optional)'), {
      target: { value: 'Need GPU for training' },
    });
    fireEvent.click(screen.getByText('Request Booking'));

    await waitFor(() => {
      expect(mockedCreateReservation).toHaveBeenCalledOnce();
      const callArgs = mockedCreateReservation.mock.calls[0][0];
      expect(callArgs.reason).toBe('Need GPU for training');
    });
  });

  it('calls onCancel when cancel button clicked', () => {
    const onCancel = vi.fn();
    renderDialog({ onCancel });

    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('shows error on API failure', async () => {
    mockedCreateReservation.mockRejectedValue(new Error('Time slot conflict'));

    renderDialog();

    fireEvent.click(screen.getByText('Request Booking'));

    await waitFor(() => {
      expect(screen.getByText('Time slot conflict')).toBeTruthy();
    });
  });

  it('shows generic error message for non-Error exceptions', async () => {
    mockedCreateReservation.mockRejectedValue('unexpected');

    renderDialog();

    fireEvent.click(screen.getByText('Request Booking'));

    await waitFor(() => {
      expect(screen.getByText('Failed to create reservation')).toBeTruthy();
    });
  });

  it('shows "Submitting..." text while submitting', async () => {
    // Create a promise that we control
    let resolvePromise: (value: { id: string; status: string }) => void;
    mockedCreateReservation.mockImplementation(
      () => new Promise((resolve) => { resolvePromise = resolve; }),
    );

    renderDialog();

    fireEvent.click(screen.getByText('Request Booking'));

    await waitFor(() => {
      expect(screen.getByText('Submitting...')).toBeTruthy();
    });

    // Resolve to clean up
    resolvePromise!({ id: 'res-1', status: 'pending' });
  });

  it('calls onCancel when dialog backdrop is clicked', () => {
    const onCancel = vi.fn();
    const { container } = renderDialog({ onCancel });

    const dialog = container.querySelector('dialog')!;
    fireEvent.click(dialog);
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
