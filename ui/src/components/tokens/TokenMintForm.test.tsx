import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { createElement } from 'react';
import type { ReactNode } from 'react';
import { ThemeProvider } from '../../theme';
import TokenMintForm from './TokenMintForm';
import { mintToken, getCategories, getUserModels } from '../../api';

vi.mock('../../api', () => ({
  mintToken: vi.fn(),
  getCategories: vi.fn(),
  getUserModels: vi.fn(),
}));

const mockedGetCategories = vi.mocked(getCategories);
const mockedGetUserModels = vi.mocked(getUserModels);
const mockedMintToken = vi.mocked(mintToken);

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

  // Default: resolve with empty arrays
  mockedGetCategories.mockResolvedValue([]);
  mockedGetUserModels.mockResolvedValue([]);
});

function wrapper({ children }: { children: ReactNode }) {
  return createElement(ThemeProvider, null, children);
}

function renderForm(props: Partial<React.ComponentProps<typeof TokenMintForm>> = {}) {
  return render(<TokenMintForm {...props} />, { wrapper });
}

describe('TokenMintForm', () => {
  it('renders the name input field', async () => {
    renderForm();

    // Wait for categories/models to finish loading
    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });
  });

  it('renders category select after loading', async () => {
    mockedGetCategories.mockResolvedValue([
      { id: 'cat-1', name: 'General', description: 'General purpose', preferred_model_id: null, created_at: '' },
    ]);

    renderForm();

    await waitFor(() => {
      expect(screen.getByLabelText('Category')).toBeTruthy();
    });

    const select = screen.getByLabelText('Category') as HTMLSelectElement;
    expect(select.querySelector('option[value="cat-1"]')).toBeTruthy();
  });

  it('renders create token button', async () => {
    renderForm();

    await waitFor(() => {
      expect(screen.getByText('Create Token')).toBeTruthy();
    });
  });

  it('disables submit button when name is empty', async () => {
    renderForm();

    await waitFor(() => {
      const btn = screen.getByText('Create Token') as HTMLButtonElement;
      expect(btn.disabled).toBe(true);
    });
  });

  it('enables submit button when name is entered', async () => {
    renderForm();

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'my-token' } });
    const btn = screen.getByText('Create Token') as HTMLButtonElement;
    expect(btn.disabled).toBe(false);
  });

  it('calls mintToken API on submit and shows minted token', async () => {
    mockedMintToken.mockResolvedValue({
      id: 'tok-1',
      token: 'sk-abc123secret',
      name: 'my-token',
      warning: '',
    });

    renderForm();

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'my-token' } });
    fireEvent.submit(screen.getByText('Create Token').closest('form')!);

    await waitFor(() => {
      expect(mockedMintToken).toHaveBeenCalledWith({
        name: 'my-token',
        category_id: null,
        specific_model_id: null,
        expires_at: null,
      });
    });

    await waitFor(() => {
      expect(screen.getByText('sk-abc123secret')).toBeTruthy();
    });

    // Should show the save warning
    expect(screen.getByText(/Save this token now/)).toBeTruthy();
    // Should show "Create Another" button
    expect(screen.getByText('Create Another')).toBeTruthy();
  });

  it('shows error on API failure', async () => {
    mockedMintToken.mockRejectedValue(new Error('Token limit exceeded'));

    renderForm();

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'my-token' } });
    fireEvent.submit(screen.getByText('Create Token').closest('form')!);

    await waitFor(() => {
      expect(screen.getByText('Token limit exceeded')).toBeTruthy();
    });
  });

  it('calls onMinted callback after successful mint', async () => {
    const onMinted = vi.fn();
    mockedMintToken.mockResolvedValue({
      id: 'tok-1',
      token: 'sk-xyz',
      name: 'test',
      warning: '',
    });

    renderForm({ onMinted });

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'test' } });
    fireEvent.submit(screen.getByText('Create Token').closest('form')!);

    await waitFor(() => {
      expect(onMinted).toHaveBeenCalledOnce();
    });
  });

  it('fetches categories and models on mount', async () => {
    renderForm();

    await waitFor(() => {
      expect(mockedGetCategories).toHaveBeenCalledOnce();
      expect(mockedGetUserModels).toHaveBeenCalledOnce();
    });
  });

  it('returns to form when "Create Another" is clicked after minting', async () => {
    mockedMintToken.mockResolvedValue({
      id: 'tok-1',
      token: 'sk-abc',
      name: 'my-token',
      warning: '',
    });

    renderForm();

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });

    fireEvent.change(screen.getByLabelText('Token Name *'), { target: { value: 'my-token' } });
    fireEvent.submit(screen.getByText('Create Token').closest('form')!);

    await waitFor(() => {
      expect(screen.getByText('Create Another')).toBeTruthy();
    });

    fireEvent.click(screen.getByText('Create Another'));

    await waitFor(() => {
      expect(screen.getByLabelText('Token Name *')).toBeTruthy();
    });
  });
});
