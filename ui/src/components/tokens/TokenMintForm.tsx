import { useState, useEffect } from 'react';
import { mintToken, getCategories, getUserModels } from '../../api';
import type { AdminModel, Category, MintedToken } from '../../types';
import { useTheme, formStyles } from '../../theme';
import LoadingSpinner from '../common/LoadingSpinner';
import ErrorAlert from '../common/ErrorAlert';
import CopyButton from '../common/CopyButton';

interface TokenMintFormProps {
  /** Called after a token is successfully minted (e.g. to refresh a token list) */
  onMinted?: () => void;
  /** HTML id prefix for form elements (for label association). Defaults to "mint" */
  idPrefix?: string;
}

export default function TokenMintForm({ onMinted, idPrefix = 'mint' }: Readonly<TokenMintFormProps>) {
  const { colors } = useTheme();
  const form = formStyles(colors);

  const [categories, setCategories] = useState<Category[]>([]);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [loadingCategories, setLoadingCategories] = useState(true);
  const [loadingModels, setLoadingModels] = useState(true);

  const [name, setName] = useState('');
  const [categoryId, setCategoryId] = useState('');
  const [specificModelId, setSpecificModelId] = useState('');
  const [expiresInDays, setExpiresInDays] = useState<number | null>(90);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [minted, setMinted] = useState<MintedToken | null>(null);

  useEffect(() => {
    (async () => {
      try {
        const cats = await getCategories();
        setCategories(cats);
      } catch {
        // Non-critical — category dropdown will just be empty
      } finally {
        setLoadingCategories(false);
      }
    })();
    (async () => {
      try {
        const m = await getUserModels();
        m.sort((a, b) => {
          if (a.loaded !== b.loaded) return a.loaded ? -1 : 1;
          return a.hf_repo.localeCompare(b.hf_repo);
        });
        setModels(m);
      } catch {
        // Non-critical — model dropdown will just be empty
      } finally {
        setLoadingModels(false);
      }
    })();
  }, []);

  const resetForm = () => {
    setName('');
    setCategoryId('');
    setSpecificModelId('');
    setExpiresInDays(90);
    setShowAdvanced(false);
  };

  const handleSubmit = async (e: React.SubmitEvent) => {
    e.preventDefault();
    if (!name.trim()) return;

    setSubmitting(true);
    setSubmitError(null);
    try {
      const result = await mintToken({
        name: name.trim(),
        category_id: categoryId || null,
        specific_model_id: specificModelId.trim() || null,
        expires_in_days: expiresInDays,
      });
      setMinted(result);
      resetForm();
      onMinted?.();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Failed to create token');
    } finally {
      setSubmitting(false);
    }
  };

  if (minted) {
    return (
      <div>
        <div
          style={{
            background: colors.warningBannerBg,
            border: `1px solid ${colors.warningBannerBorder}`,
            borderRadius: 8,
            padding: '1.25rem',
            marginBottom: '1rem',
          }}
        >
          <p style={{ margin: '0 0 0.5rem', fontWeight: 600, color: colors.warningBannerText }}>
            Save this token now — it will not be shown again.
          </p>
          <div
            style={{
              background: colors.cardBg,
              border: `1px solid ${colors.cardBorder}`,
              borderRadius: 4,
              padding: '0.75rem',
              fontFamily: 'monospace',
              fontSize: '0.9rem',
              wordBreak: 'break-all',
              display: 'flex',
              alignItems: 'center',
              gap: '0.75rem',
            }}
          >
            <code style={{ flex: 1, color: colors.textPrimary }}>{minted.token}</code>
            <CopyButton text={minted.token} />
          </div>
          <p style={{ margin: '0.5rem 0 0', color: colors.textMuted, fontSize: '0.85rem' }}>
            Token name: <strong>{minted.name}</strong>
          </p>
        </div>
        <button
          onClick={() => setMinted(null)}
          style={{
            padding: '0.5rem 1rem',
            background: colors.buttonPrimary,
            color: '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          Create Another
        </button>
      </div>
    );
  }

  return (
    <div>
      {submitError && <ErrorAlert message={submitError} />}
      <form onSubmit={handleSubmit}>
        <div style={{ marginBottom: '1rem' }}>
          <label htmlFor={`${idPrefix}-token-name`} style={form.label}>Token Name *</label>
          <input
            id={`${idPrefix}-token-name`}
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. my-project-key"
            style={form.input}
            required
          />
        </div>

        <div style={{ marginBottom: '1rem' }}>
          <label htmlFor={`${idPrefix}-token-category`} style={form.label}>Category</label>
          {loadingCategories ? (
            <LoadingSpinner message="Loading categories..." />
          ) : (
            <select
              id={`${idPrefix}-token-category`}
              value={categoryId}
              onChange={(e) => setCategoryId(e.target.value)}
              style={{ ...form.input, background: colors.inputBg }}
            >
              <option value="">No category restriction</option>
              {categories.map((cat) => (
                <option key={cat.id} value={cat.id}>
                  {cat.name} — {cat.description}
                </option>
              ))}
            </select>
          )}
        </div>

        <div style={{ marginBottom: '1rem' }}>
          <label htmlFor={`${idPrefix}-token-expiry`} style={form.label}>Expires In</label>
          <select
            id={`${idPrefix}-token-expiry`}
            value={expiresInDays ?? ''}
            onChange={(e) => setExpiresInDays(e.target.value === '' ? null : Number(e.target.value))}
            style={{ ...form.input, background: colors.inputBg }}
          >
            <option value="30">30 days</option>
            <option value="60">60 days</option>
            <option value="90">90 days</option>
            <option value="180">180 days</option>
            <option value="365">1 year</option>
          </select>
        </div>

        <div style={{ marginBottom: '1.25rem' }}>
          <button
            type="button"
            onClick={() => setShowAdvanced(!showAdvanced)}
            style={{
              background: 'none',
              border: 'none',
              color: colors.link,
              cursor: 'pointer',
              fontSize: '0.85rem',
              padding: 0,
              textDecoration: 'underline',
            }}
          >
            {showAdvanced ? 'Hide' : 'Show'} Advanced Options
          </button>

          {showAdvanced && (
            <div style={{ marginTop: '0.75rem', paddingLeft: '0.5rem', borderLeft: `2px solid ${colors.cardBorder}` }}>
              <div style={{ marginBottom: '1rem' }}>
                <label htmlFor={`${idPrefix}-token-model`} style={form.label}>Specific Model</label>
                {loadingModels ? (
                  <LoadingSpinner message="Loading models..." />
                ) : (
                  <select
                    id={`${idPrefix}-token-model`}
                    value={specificModelId}
                    onChange={(e) => setSpecificModelId(e.target.value)}
                    style={{ ...form.input, background: colors.inputBg }}
                  >
                    <option value="">No model restriction</option>
                    {(() => {
                      const loaded = models.filter((m) => m.loaded);
                      const unloaded = models.filter((m) => !m.loaded);
                      return (
                        <>
                          {loaded.map((m) => (
                            <option key={m.id} value={m.id}>{m.hf_repo}</option>
                          ))}
                          {unloaded.length > 0 && (
                            <>
                              <option disabled>── Not loaded ──</option>
                              {unloaded.map((m) => (
                                <option key={m.id} value={m.id}>{m.hf_repo}</option>
                              ))}
                            </>
                          )}
                        </>
                      );
                    })()}
                  </select>
                )}
                <small style={{ color: colors.textMuted, fontSize: '0.8rem' }}>
                  Restrict this token to a specific model. Leave blank to use category routing.
                </small>
              </div>
            </div>
          )}
        </div>

        <button
          type="submit"
          disabled={submitting || !name.trim()}
          style={{
            padding: '0.6rem 1.5rem',
            background: submitting ? colors.buttonPrimaryDisabled : colors.buttonPrimary,
            color: '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: submitting ? 'default' : 'pointer',
            fontSize: '1rem',
          }}
        >
          {submitting ? 'Creating...' : 'Create Token'}
        </button>
      </form>
    </div>
  );
}
