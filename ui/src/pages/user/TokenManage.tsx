import { useState, useEffect, useCallback } from 'react';
import { getUserTokens, revokeToken, mintToken, getCategories, getUserModels } from '../../api';
import type { UserToken, AdminModel, Category, MintedToken } from '../../types';
import { useTheme } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';
import CopyButton from '../../components/common/CopyButton';

export default function TokenManage() {
  const { colors } = useTheme();
  const [tokens, setTokens] = useState<UserToken[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revoking, setRevoking] = useState<string | null>(null);
  const [confirmRevoke, setConfirmRevoke] = useState<string | null>(null);

  // Create form state
  const [showForm, setShowForm] = useState(false);
  const [categories, setCategories] = useState<Category[]>([]);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [loadingCategories, setLoadingCategories] = useState(false);
  const [loadingModels, setLoadingModels] = useState(false);
  const [name, setName] = useState('');
  const [categoryId, setCategoryId] = useState('');
  const [specificModelId, setSpecificModelId] = useState('');
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);
  const [minted, setMinted] = useState<MintedToken | null>(null);

  const tableStyle: React.CSSProperties = {
    width: '100%',
    borderCollapse: 'collapse',
    background: colors.tableBg,
    borderRadius: 8,
    overflow: 'hidden',
    border: `1px solid ${colors.cardBorder}`,
  };

  const thStyle: React.CSSProperties = {
    textAlign: 'left',
    padding: '0.75rem 1rem',
    background: colors.tableHeaderBg,
    borderBottom: `1px solid ${colors.cardBorder}`,
    fontSize: '0.85rem',
    fontWeight: 600,
    color: colors.tableHeaderText,
    textTransform: 'uppercase',
    letterSpacing: '0.03em',
  };

  const tdStyle: React.CSSProperties = {
    padding: '0.75rem 1rem',
    borderBottom: `1px solid ${colors.tableRowBorder}`,
    fontSize: '0.9rem',
  };

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

  const fetchTokens = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getUserTokens();
      setTokens(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load tokens');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchTokens();
  }, [fetchTokens]);

  const handleRevoke = async (id: string) => {
    setConfirmRevoke(null);
    setRevoking(id);
    try {
      await revokeToken(id);
      setTokens((prev) => prev.map((t) => (t.id === id ? { ...t, revoked: true } : t)));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to revoke token');
    } finally {
      setRevoking(null);
    }
  };

  const openForm = async () => {
    setShowForm(true);
    setLoadingCategories(true);
    setLoadingModels(true);
    try {
      const cats = await getCategories();
      setCategories(cats);
    } catch {
      // Non-critical
    } finally {
      setLoadingCategories(false);
    }
    try {
      const m = await getUserModels();
      m.sort((a, b) => {
        if (a.loaded !== b.loaded) return a.loaded ? -1 : 1;
        return a.hf_repo.localeCompare(b.hf_repo);
      });
      setModels(m);
    } catch {
      // Non-critical
    } finally {
      setLoadingModels(false);
    }
  };

  const closeForm = () => {
    setShowForm(false);
    setName('');
    setCategoryId('');
    setSpecificModelId('');
    setShowAdvanced(false);
    setSubmitError(null);
    setMinted(null);
  };

  const handleCreate = async (e: React.SubmitEvent) => {
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
      setName('');
      setCategoryId('');
      setSpecificModelId('');
      setShowAdvanced(false);
      fetchTokens();
    } catch (err) {
      setSubmitError(err instanceof Error ? err.message : 'Failed to create token');
    } finally {
      setSubmitting(false);
    }
  };

  if (loading) return <LoadingSpinner message="Loading tokens..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchTokens} />;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0 }}>API Tokens</h1>
        <button
          onClick={() => showForm ? closeForm() : openForm()}
          style={{
            padding: '0.5rem 1rem',
            background: showForm ? colors.buttonDisabled : colors.buttonPrimary,
            color: showForm ? colors.textSecondary : '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          {showForm ? 'Cancel' : 'Create Token'}
        </button>
      </div>

      {/* Minted token banner */}
      {minted && (
        <div
          style={{
            background: colors.warningBannerBg,
            border: `1px solid ${colors.warningBannerBorder}`,
            borderRadius: 8,
            padding: '1.25rem',
            marginBottom: '1.5rem',
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
      )}

      {/* Create form */}
      {showForm && !minted && (
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1.5rem', marginBottom: '1.5rem', maxWidth: 500 }}>
          <h3 style={{ margin: '0 0 1rem' }}>New Token</h3>
          {submitError && <ErrorAlert message={submitError} />}
          <form onSubmit={handleCreate}>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="manage-token-name" style={labelStyle}>Token Name *</label>
              <input
                id="manage-token-name"
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="e.g. my-project-key"
                style={inputStyle}
                required
              />
            </div>

            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="manage-token-category" style={labelStyle}>Category</label>
              {loadingCategories ? (
                <LoadingSpinner message="Loading categories..." />
              ) : (
                <select
                  id="manage-token-category"
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
                    <label htmlFor="manage-token-model" style={labelStyle}>Specific Model</label>
                    {loadingModels ? (
                      <LoadingSpinner message="Loading models..." />
                    ) : (
                      <select
                        id="manage-token-model"
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
      )}

      {tokens.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No tokens created yet.</p>
      ) : (
        <table style={tableStyle}>
          <thead>
            <tr>
              <th style={thStyle}>Name</th>
              <th style={thStyle}>Category</th>
              <th style={thStyle}>Created</th>
              <th style={thStyle}>Status</th>
              <th style={thStyle}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {tokens.map((token) => (
              <tr key={token.id}>
                <td style={tdStyle}>{token.name}</td>
                <td style={tdStyle}>{token.category_name || '\u2014'}</td>
                <td style={tdStyle}>{new Date(token.created_at).toLocaleString()}</td>
                <td style={tdStyle}>
                  <span
                    style={{
                      display: 'inline-block',
                      padding: '0.2rem 0.6rem',
                      borderRadius: 12,
                      fontSize: '0.8rem',
                      fontWeight: 600,
                      background: token.revoked ? colors.badgeDangerBg : colors.badgeSuccessBg,
                      color: token.revoked ? colors.badgeDangerText : colors.badgeSuccessText,
                    }}
                  >
                    {token.revoked ? 'Revoked' : 'Active'}
                  </span>
                </td>
                <td style={tdStyle}>
                  {!token.revoked && (
                    <button
                      onClick={() => setConfirmRevoke(token.id)}
                      disabled={revoking === token.id}
                      style={{
                        padding: '0.3rem 0.7rem',
                        background: colors.buttonDanger,
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        cursor: revoking === token.id ? 'default' : 'pointer',
                        fontSize: '0.8rem',
                        opacity: revoking === token.id ? 0.5 : 1,
                      }}
                    >
                      {revoking === token.id ? 'Revoking...' : 'Revoke'}
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirmRevoke && (
        <ConfirmDialog
          title="Revoke Token"
          message="This token will immediately stop working. This action cannot be undone."
          confirmLabel="Revoke"
          destructive
          onConfirm={() => handleRevoke(confirmRevoke)}
          onCancel={() => setConfirmRevoke(null)}
        />
      )}
    </div>
  );
}
