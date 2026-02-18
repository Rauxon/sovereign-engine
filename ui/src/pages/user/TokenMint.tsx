import { useState, useEffect } from 'react';
import { mintToken, getCategories, getUserModels } from '../../api';
import type { AdminModel, Category, MintedToken } from '../../types';
import { useTheme } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import CopyButton from '../../components/common/CopyButton';

export default function TokenMint() {
  const { colors } = useTheme();
  const [categories, setCategories] = useState<Category[]>([]);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [loadingCategories, setLoadingCategories] = useState(true);
  const [loadingModels, setLoadingModels] = useState(true);
  const [catError, setCatError] = useState<string | null>(null);

  const [name, setName] = useState('');
  const [categoryId, setCategoryId] = useState<string>('');
  const [specificModelId, setSpecificModelId] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);

  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [minted, setMinted] = useState<MintedToken | null>(null);

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '0.5rem 0.75rem',
    border: `1px solid ${colors.inputBorder}`,
    borderRadius: 4,
    fontSize: '0.95rem',
    boxSizing: 'border-box',
    background: colors.inputBg,
    color: colors.textPrimary,
  };

  const labelStyle: React.CSSProperties = {
    display: 'block',
    marginBottom: '0.35rem',
    fontWeight: 600,
    fontSize: '0.9rem',
    color: colors.textSecondary,
  };

  useEffect(() => {
    (async () => {
      try {
        const cats = await getCategories();
        setCategories(cats);
      } catch (err) {
        setCatError(err instanceof Error ? err.message : 'Failed to load categories');
      } finally {
        setLoadingCategories(false);
      }
    })();
    (async () => {
      try {
        const m = await getUserModels();
        // Sort: loaded first, then alphabetical by hf_repo
        m.sort((a, b) => {
          if (a.loaded !== b.loaded) return a.loaded ? -1 : 1;
          return a.hf_repo.localeCompare(b.hf_repo);
        });
        setModels(m);
      } catch {
        // Non-critical — the dropdown will just be empty
      } finally {
        setLoadingModels(false);
      }
    })();
  }, []);

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
        expires_at: null,
      });
      setMinted(result);
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Failed to create token');
    } finally {
      setSubmitting(false);
    }
  };

  if (minted) {
    return (
      <div>
        <h1>Token Created</h1>
        <div
          style={{
            background: colors.warningBannerBg,
            border: `1px solid ${colors.warningBannerBorder}`,
            borderRadius: 8,
            padding: '1.25rem',
            marginBottom: '1rem',
            maxWidth: 600,
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
        </div>
        <p style={{ color: colors.textMuted }}>
          Token name: <strong>{minted.name}</strong>
        </p>
        <button
          onClick={() => {
            setMinted(null);
            setName('');
            setCategoryId('');
            setSpecificModelId('');
          }}
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
    <div style={{ maxWidth: 500 }}>
      <h1>Mint Token</h1>

      {catError && <ErrorAlert message={catError} />}
      {submitError && <ErrorAlert message={submitError} />}

      <form onSubmit={handleSubmit}>
        <div style={{ marginBottom: '1rem' }}>
          <label htmlFor="mint-token-name" style={labelStyle}>Token Name *</label>
          <input
            id="mint-token-name"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. my-project-key"
            style={inputStyle}
            required
          />
        </div>

        <div style={{ marginBottom: '1rem' }}>
          <label htmlFor="mint-token-category" style={labelStyle}>Category</label>
          {loadingCategories ? (
            <LoadingSpinner message="Loading categories..." />
          ) : (
            <select
              id="mint-token-category"
              value={categoryId}
              onChange={(e) => setCategoryId(e.target.value)}
              style={{ ...inputStyle, background: colors.inputBg }}
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
                <label htmlFor="mint-token-model" style={labelStyle}>Specific Model</label>
                {loadingModels ? (
                  <LoadingSpinner message="Loading models..." />
                ) : (
                  <select
                    id="mint-token-model"
                    value={specificModelId}
                    onChange={(e) => setSpecificModelId(e.target.value)}
                    style={{ ...inputStyle, background: colors.inputBg }}
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
