import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import ConfirmDialog from './ConfirmDialog';

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

  // jsdom does not implement HTMLDialogElement methods
  HTMLDialogElement.prototype.showModal = vi.fn();
  HTMLDialogElement.prototype.close = vi.fn();
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

function renderDialog(props: Partial<React.ComponentProps<typeof ConfirmDialog>> = {}) {
  const defaults = {
    title: 'Test Title',
    message: 'Test message body',
    onConfirm: vi.fn(),
    onCancel: vi.fn(),
  };
  const merged = { ...defaults, ...props };
  const result = render(<ConfirmDialog {...merged} />, { wrapper });
  return { ...result, ...merged };
}

describe('ConfirmDialog', () => {
  it('renders title and message', () => {
    renderDialog({ title: 'Delete Item?', message: 'This cannot be undone.' });

    expect(screen.getByText('Delete Item?')).toBeTruthy();
    expect(screen.getByText('This cannot be undone.')).toBeTruthy();
  });

  it('calls showModal on mount', () => {
    renderDialog();
    expect(HTMLDialogElement.prototype.showModal).toHaveBeenCalled();
  });

  it('calls onConfirm when confirm button clicked', () => {
    const onConfirm = vi.fn();
    renderDialog({ onConfirm });

    fireEvent.click(screen.getByText('Confirm'));
    expect(onConfirm).toHaveBeenCalledOnce();
  });

  it('calls onCancel when cancel button clicked', () => {
    const onCancel = vi.fn();
    renderDialog({ onCancel });

    fireEvent.click(screen.getByText('Cancel'));
    expect(onCancel).toHaveBeenCalledOnce();
  });

  it('shows default "Confirm" label when confirmLabel not provided', () => {
    renderDialog();
    expect(screen.getByText('Confirm')).toBeTruthy();
  });

  it('shows custom confirmLabel', () => {
    renderDialog({ confirmLabel: 'Yes, Delete' });
    expect(screen.getByText('Yes, Delete')).toBeTruthy();
    // The default "Confirm" should not be present
    expect(screen.queryByText('Confirm')).toBeNull();
  });

  it('uses danger styling when destructive=true', () => {
    renderDialog({ destructive: true, confirmLabel: 'Delete' });

    const confirmBtn = screen.getByText('Delete');
    // The destructive button should use buttonDanger color (lightColors.buttonDanger = '#dc2626')
    expect(confirmBtn.style.background).toBe('rgb(220, 38, 38)');
  });

  it('uses primary styling when destructive=false (default)', () => {
    renderDialog();

    const confirmBtn = screen.getByText('Confirm');
    // The non-destructive button should use buttonPrimary color (lightColors.buttonPrimary = '#2563eb')
    expect(confirmBtn.style.background).toBe('rgb(37, 99, 235)');
  });

  it('calls onCancel when dialog backdrop is clicked', () => {
    const onCancel = vi.fn();
    const { container } = renderDialog({ onCancel });

    const dialog = container.querySelector('dialog')!;
    // Simulate clicking the dialog itself (backdrop click) where target === currentTarget
    fireEvent.click(dialog);
    expect(onCancel).toHaveBeenCalledOnce();
  });
});
