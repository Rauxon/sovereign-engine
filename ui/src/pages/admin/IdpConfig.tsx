import { useState, useEffect, useCallback } from 'react';
import { getIdps, createIdp, updateIdp, deleteIdp } from '../../api';
import type { IdP } from '../../types';
import { useTheme, tableStyles, formStyles } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';

export default function IdpConfig() {
  const { colors } = useTheme();
  const [idps, setIdps] = useState<IdP[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [confirmDisable, setConfirmDisable] = useState<string | null>(null);

  // Form state
  const [formName, setFormName] = useState('');
  const [formIssuer, setFormIssuer] = useState('');
  const [formClientId, setFormClientId] = useState('');
  const [formClientSecret, setFormClientSecret] = useState('');
  const [formScopes, setFormScopes] = useState('openid email profile');
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const { table: tableStyle, th: thStyle, td: tdStyle } = tableStyles(colors);
  const { input: inputStyle, label: labelStyle } = formStyles(colors);

  const fetchIdps = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getIdps();
      setIdps(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load identity providers');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchIdps();
  }, [fetchIdps]);

  const handleSubmit = async (e: React.SubmitEvent) => {
    e.preventDefault();
    setSubmitting(true);
    setSubmitError(null);
    try {
      if (editingId) {
        const updates: Record<string, string> = {
          name: formName.trim(),
          issuer: formIssuer.trim(),
          client_id: formClientId.trim(),
          scopes: formScopes.trim(),
        };
        if (formClientSecret.trim()) {
          updates.client_secret = formClientSecret.trim();
        }
        await updateIdp(editingId, updates);
      } else {
        await createIdp({
          name: formName.trim(),
          issuer: formIssuer.trim(),
          client_id: formClientId.trim(),
          client_secret: formClientSecret.trim(),
          scopes: formScopes.trim(),
        });
      }
      setShowForm(false);
      setEditingId(null);
      resetForm();
      await fetchIdps();
    } catch (err) {
      const fallback = editingId ? 'Failed to update IdP' : 'Failed to create IdP';
      setSubmitError(err instanceof Error ? err.message : fallback);
    } finally {
      setSubmitting(false);
    }
  };

  const handleDisable = async (id: string) => {
    setConfirmDisable(null);
    try {
      await deleteIdp(id);
      setIdps((prev) => prev.map((idp) => (idp.id === id ? { ...idp, enabled: false } : idp)));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to disable IdP');
    }
  };

  const handleEnable = async (id: string) => {
    try {
      await updateIdp(id, { enabled: true });
      setIdps((prev) => prev.map((idp) => (idp.id === id ? { ...idp, enabled: true } : idp)));
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to enable IdP');
    }
  };

  const startEdit = (idp: IdP) => {
    setEditingId(idp.id);
    setFormName(idp.name);
    setFormIssuer(idp.issuer);
    setFormClientId(idp.client_id);
    setFormClientSecret('');
    setFormScopes(idp.scopes);
    setSubmitError(null);
    setShowForm(true);
  };

  const resetForm = () => {
    setFormName('');
    setFormIssuer('');
    setFormClientId('');
    setFormClientSecret('');
    setFormScopes('openid email profile');
    setEditingId(null);
    setSubmitError(null);
  };

  const getSubmitButtonLabel = () => {
    if (submitting) return editingId ? 'Saving...' : 'Creating...';
    return editingId ? 'Save Changes' : 'Create IdP';
  };

  if (loading) return <LoadingSpinner message="Loading identity providers..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchIdps} />;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0 }}>Identity Providers</h1>
        <button
          onClick={() => { if (showForm) { setShowForm(false); resetForm(); } else { resetForm(); setShowForm(true); } }}
          style={{
            padding: '0.5rem 1rem',
            background: showForm ? colors.buttonDisabled : colors.buttonPrimary,
            color: showForm ? colors.textSecondary : '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
          }}
        >
          {showForm ? 'Cancel' : 'Add IdP'}
        </button>
      </div>

      {showForm && (
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1.5rem', marginBottom: '1.5rem', maxWidth: 500 }}>
          <h3 style={{ margin: '0 0 1rem' }}>{editingId ? 'Edit Identity Provider' : 'New Identity Provider'}</h3>
          {submitError && <ErrorAlert message={submitError} />}
          <form onSubmit={handleSubmit}>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="idp-name" style={labelStyle}>Name *</label>
              <input id="idp-name" type="text" value={formName} onChange={(e) => setFormName(e.target.value)} style={inputStyle} placeholder="e.g. Google Workspace" required />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="idp-issuer" style={labelStyle}>Issuer URL *</label>
              <input id="idp-issuer" type="url" value={formIssuer} onChange={(e) => setFormIssuer(e.target.value)} style={inputStyle} placeholder="https://accounts.google.com" required />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="idp-client-id" style={labelStyle}>Client ID *</label>
              <input id="idp-client-id" type="text" value={formClientId} onChange={(e) => setFormClientId(e.target.value)} style={inputStyle} required />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="idp-client-secret" style={labelStyle}>Client Secret {editingId ? '(leave blank to keep current)' : '*'}</label>
              <input id="idp-client-secret" type="password" value={formClientSecret} onChange={(e) => setFormClientSecret(e.target.value)} style={inputStyle} required={!editingId} />
            </div>
            <div style={{ marginBottom: '1rem' }}>
              <label htmlFor="idp-scopes" style={labelStyle}>Scopes</label>
              <input id="idp-scopes" type="text" value={formScopes} onChange={(e) => setFormScopes(e.target.value)} style={inputStyle} />
            </div>
            <button
              type="submit"
              disabled={submitting}
              style={{
                padding: '0.5rem 1.25rem',
                background: submitting ? colors.buttonPrimaryDisabled : colors.buttonPrimary,
                color: '#fff',
                border: 'none',
                borderRadius: 4,
                cursor: submitting ? 'default' : 'pointer',
              }}
            >
              {getSubmitButtonLabel()}
            </button>
          </form>
        </div>
      )}

      {idps.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No identity providers configured.</p>
      ) : (
        <table style={tableStyle}>
          <thead>
            <tr>
              <th style={thStyle}>Name</th>
              <th style={thStyle}>Issuer</th>
              <th style={thStyle}>Client ID</th>
              <th style={thStyle}>Scopes</th>
              <th style={thStyle}>Status</th>
              <th style={thStyle}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {idps.map((idp) => (
              <tr key={idp.id}>
                <td style={tdStyle}>{idp.name}</td>
                <td style={{ ...tdStyle, maxWidth: 250, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{idp.issuer}</td>
                <td style={{ ...tdStyle, fontFamily: 'monospace', fontSize: '0.8rem' }}>{idp.client_id}</td>
                <td style={tdStyle}>{idp.scopes}</td>
                <td style={tdStyle}>
                  <span
                    style={{
                      display: 'inline-block',
                      padding: '0.2rem 0.6rem',
                      borderRadius: 12,
                      fontSize: '0.8rem',
                      fontWeight: 600,
                      background: idp.enabled ? colors.badgeSuccessBg : colors.badgeDangerBg,
                      color: idp.enabled ? colors.badgeSuccessText : colors.badgeDangerText,
                    }}
                  >
                    {idp.enabled ? 'Enabled' : 'Disabled'}
                  </span>
                </td>
                <td style={{ ...tdStyle, display: 'flex', gap: '0.4rem' }}>
                  <button
                    onClick={() => startEdit(idp)}
                    style={{
                      padding: '0.3rem 0.7rem',
                      background: colors.buttonPrimary,
                      color: '#fff',
                      border: 'none',
                      borderRadius: 4,
                      cursor: 'pointer',
                      fontSize: '0.8rem',
                    }}
                  >
                    Edit
                  </button>
                  {idp.enabled ? (
                    <button
                      onClick={() => setConfirmDisable(idp.id)}
                      style={{
                        padding: '0.3rem 0.7rem',
                        background: colors.buttonDanger,
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        cursor: 'pointer',
                        fontSize: '0.8rem',
                      }}
                    >
                      Disable
                    </button>
                  ) : (
                    <button
                      onClick={() => handleEnable(idp.id)}
                      style={{
                        padding: '0.3rem 0.7rem',
                        background: colors.successText,
                        color: '#fff',
                        border: 'none',
                        borderRadius: 4,
                        cursor: 'pointer',
                        fontSize: '0.8rem',
                      }}
                    >
                      Enable
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirmDisable && (
        <ConfirmDialog
          title="Disable Identity Provider"
          message="Users will no longer be able to log in via this provider. Existing sessions will not be affected."
          confirmLabel="Disable"
          destructive
          onConfirm={() => handleDisable(confirmDisable)}
          onCancel={() => setConfirmDisable(null)}
        />
      )}
    </div>
  );
}
