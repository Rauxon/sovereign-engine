import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import TokenManage from './TokenManage';
import { getUserTokens, revokeToken, deleteToken, mintToken, getCategories, getUserModels } from '../../api';
import type { UserToken } from '../../types';

vi.mock('../../api', () => ({
  getUserTokens: vi.fn(),
  revokeToken: vi.fn(),
  deleteToken: vi.fn(),
  mintToken: vi.fn(),
  getCategories: vi.fn(),
  getUserModels: vi.fn(),
}));

const mockedGetUserTokens = vi.mocked(getUserTokens);
const mockedRevokeToken = vi.mocked(revokeToken);
const mockedDeleteToken = vi.mocked(deleteToken);
const mockedMintToken = vi.mocked(mintToken);
const mockedGetCategories = vi.mocked(getCategories);
const mockedGetUserModels = vi.mocked(getUserModels);

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

  // Default: mint form dependencies resolve with empty arrays
  mockedGetCategories.mockResolvedValue([]);
  mockedGetUserModels.mockResolvedValue([]);
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

function renderPage() {
  return render(<TokenManage />, { wrapper });
}

/** A future date guaranteed to be in the future for test purposes. */
const futureDate = '2099-12-31T23:59:59';
/** A past date guaranteed to be expired. */
const pastDate = '2020-01-01T00:00:00';

function makeToken(overrides: Partial<UserToken> = {}): UserToken {
  return {
    id: 'tok-1',
    name: 'my-token',
    category_id: null,
    category_name: null,
    specific_model_id: null,
    expires_at: futureDate,
    revoked: false,
    created_at: '2025-06-01T00:00:00Z',
    ...overrides,
  };
}

describe('TokenManage', () => {
  it('shows loading spinner initially', () => {
    // Never resolve to keep in loading state
    mockedGetUserTokens.mockReturnValue(new Promise(() => {}));
    renderPage();

    expect(screen.getByText('Loading tokens...')).toBeTruthy();
  });

  it('renders token list with Expires column header', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken()]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Expires')).toBeTruthy();
    });
  });

  it('renders token name in the table', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken({ name: 'dev-key' })]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('dev-key')).toBeTruthy();
    });
  });

  it('shows "Active" badge for non-revoked, non-expired tokens', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken()]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Active')).toBeTruthy();
    });
  });

  it('shows "Expired" badge for tokens with past expiry date', async () => {
    mockedGetUserTokens.mockResolvedValue([
      makeToken({ id: 'tok-expired', expires_at: pastDate }),
    ]);
    renderPage();

    await waitFor(() => {
      // "Expired" appears twice: once in the expiry column cell, once in the status badge span
      const expiredElements = screen.getAllByText('Expired');
      expect(expiredElements.length).toBe(2);

      // The status badge is a <span> element
      const badge = expiredElements.find((el) => el.tagName === 'SPAN');
      expect(badge).toBeTruthy();
    });
  });

  it('shows "Revoked" badge for revoked tokens', async () => {
    mockedGetUserTokens.mockResolvedValue([
      makeToken({ id: 'tok-revoked', revoked: true }),
    ]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Revoked')).toBeTruthy();
    });
  });

  it('shows "Never" for tokens with no expiry', async () => {
    mockedGetUserTokens.mockResolvedValue([
      makeToken({ expires_at: null }),
    ]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Never')).toBeTruthy();
    });
  });

  it('shows empty state when no tokens exist', async () => {
    mockedGetUserTokens.mockResolvedValue([]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('No tokens created yet.')).toBeTruthy();
    });
  });

  // ---- Delete flow ----

  it('delete button triggers confirmation dialog', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken()]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Delete')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(screen.getByText('Delete Token')).toBeTruthy();
      expect(screen.getByText(/permanently remove the token/)).toBeTruthy();
    });
  });

  it('confirming delete calls deleteToken API and removes token from list', async () => {
    mockedDeleteToken.mockResolvedValue(undefined);
    mockedGetUserTokens.mockResolvedValue([
      makeToken({ id: 'tok-to-delete', name: 'delete-me' }),
    ]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('delete-me')).toBeTruthy();
    });

    // Click delete button on the row
    fireEvent.click(screen.getByText('Delete'));

    // Confirm in the dialog
    await waitFor(() => {
      expect(screen.getByText('Delete Token')).toBeTruthy();
    });

    // The confirm dialog has a button with the confirmLabel "Delete" — but there
    // are now two "Delete" texts (the row button + the dialog confirm button).
    // The dialog confirm button is inside the dialog element.
    const dialog = document.querySelector('dialog')!;
    const confirmBtn = Array.from(dialog.querySelectorAll('button')).find(
      (btn) => btn.textContent === 'Delete',
    )!;
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mockedDeleteToken).toHaveBeenCalledWith('tok-to-delete');
    });

    // Token should be removed from the list
    await waitFor(() => {
      expect(screen.queryByText('delete-me')).toBeNull();
    });
  });

  it('cancelling delete dialog does not call deleteToken', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken()]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Delete')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(screen.getByText('Delete Token')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Cancel'));

    expect(mockedDeleteToken).not.toHaveBeenCalled();
  });

  // ---- Revoke flow ----

  it('revoke button triggers confirmation dialog', async () => {
    mockedGetUserTokens.mockResolvedValue([makeToken()]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Revoke')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Revoke'));

    await waitFor(() => {
      expect(screen.getByText('Revoke Token')).toBeTruthy();
      expect(screen.getByText(/immediately stop working/)).toBeTruthy();
    });
  });

  it('confirming revoke calls revokeToken API and updates badge to Revoked', async () => {
    mockedRevokeToken.mockResolvedValue(undefined);
    mockedGetUserTokens.mockResolvedValue([makeToken({ id: 'tok-rev' })]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Revoke')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Revoke'));

    await waitFor(() => {
      expect(screen.getByText('Revoke Token')).toBeTruthy();
    });

    // Confirm in dialog — the dialog's confirm button says "Revoke"
    const dialog = document.querySelector('dialog')!;
    const confirmBtn = Array.from(dialog.querySelectorAll('button')).find(
      (btn) => btn.textContent === 'Revoke',
    )!;
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(mockedRevokeToken).toHaveBeenCalledWith('tok-rev');
    });

    // Badge should change to Revoked
    await waitFor(() => {
      expect(screen.getByText('Revoked')).toBeTruthy();
    });
  });

  it('does not show Revoke button for already-revoked tokens', async () => {
    mockedGetUserTokens.mockResolvedValue([
      makeToken({ revoked: true }),
    ]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Revoked')).toBeTruthy();
    });

    // There should be no Revoke button, only Delete
    expect(screen.queryByText('Revoke')).toBeNull();
    expect(screen.getByText('Delete')).toBeTruthy();
  });

  // ---- Create Token form ----

  it('toggles the mint form when Create Token button is clicked', async () => {
    mockedGetUserTokens.mockResolvedValue([]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Create Token')).toBeTruthy();
    });

    // Form should not be visible yet
    expect(screen.queryByText('New Token')).toBeNull();

    fireEvent.click(screen.getByText('Create Token'));

    await waitFor(() => {
      expect(screen.getByText('New Token')).toBeTruthy();
    });
  });

  it('form stays mounted after minting (initialLoading vs background refresh)', async () => {
    // Start with one existing token
    const existingToken = makeToken({ id: 'tok-existing', name: 'existing-key' });
    const newToken = makeToken({ id: 'tok-new', name: 'new-key' });

    // First call: initial load returns existing token
    // Second call: after mint, fetchTokens returns both tokens
    mockedGetUserTokens
      .mockResolvedValueOnce([existingToken])
      .mockResolvedValueOnce([existingToken, newToken]);

    mockedMintToken.mockResolvedValue({
      token: 'sk-new-secret',
      name: 'new-key',
      warning: '',
    });

    renderPage();

    // Wait for initial load
    await waitFor(() => {
      expect(screen.getByText('existing-key')).toBeTruthy();
    });

    // Open the form
    fireEvent.click(screen.getByText('Create Token'));

    await waitFor(() => {
      expect(screen.getByText('New Token')).toBeTruthy();
    });

    // Wait for form to finish loading categories/models
    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    // Fill in the form and submit
    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'new-key' } });

    // Find the Create Token button inside the form (not the top-level toggle)
    const form = screen.getByLabelText('Token Name *').closest('form')!;
    fireEvent.submit(form);

    // After minting, the form should show the minted token (not unmount)
    await waitFor(() => {
      expect(screen.getByText('sk-new-secret')).toBeTruthy();
    });

    // The form area (New Token header) should still be visible — form did not unmount
    expect(screen.getByText('New Token')).toBeTruthy();

    // fetchTokens should have been called twice: once initially, once after minting
    expect(mockedGetUserTokens).toHaveBeenCalledTimes(2);
  });

  // ---- Error handling ----

  it('shows error alert when initial fetch fails', async () => {
    mockedGetUserTokens.mockRejectedValue(new Error('Network error'));
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('Network error')).toBeTruthy();
    });
  });

  it('shows error when delete fails but keeps token in list', async () => {
    mockedDeleteToken.mockRejectedValue(new Error('Forbidden'));
    mockedGetUserTokens.mockResolvedValue([makeToken({ id: 'tok-keep', name: 'keep-me' })]);
    renderPage();

    await waitFor(() => {
      expect(screen.getByText('keep-me')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(screen.getByText('Delete Token')).toBeTruthy();
    });

    const dialog = document.querySelector('dialog')!;
    const confirmBtn = Array.from(dialog.querySelectorAll('button')).find(
      (btn) => btn.textContent === 'Delete',
    )!;
    fireEvent.click(confirmBtn);

    // The error handler uses err.message when err is an Error instance
    await waitFor(() => {
      expect(screen.getByText('Forbidden')).toBeTruthy();
    });

    // Token should still be in the list
    expect(screen.getByText('keep-me')).toBeTruthy();
  });
});
