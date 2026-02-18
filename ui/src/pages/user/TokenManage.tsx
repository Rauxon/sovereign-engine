import { useState, useEffect, useCallback } from 'react';
import { getUserTokens, revokeToken } from '../../api';
import type { UserToken } from '../../types';
import { useTheme, tableStyles } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';
import TokenMintForm from '../../components/tokens/TokenMintForm';

export default function TokenManage() {
  const { colors } = useTheme();
  const table = tableStyles(colors);
  const [tokens, setTokens] = useState<UserToken[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revoking, setRevoking] = useState<string | null>(null);
  const [confirmRevoke, setConfirmRevoke] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);

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

  if (loading) return <LoadingSpinner message="Loading tokens..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchTokens} />;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0 }}>API Tokens</h1>
        <button
          onClick={() => setShowForm(!showForm)}
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

      {showForm && (
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1.5rem', marginBottom: '1.5rem', maxWidth: 500 }}>
          <h3 style={{ margin: '0 0 1rem' }}>New Token</h3>
          <TokenMintForm idPrefix="manage" onMinted={fetchTokens} />
        </div>
      )}

      {tokens.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No tokens created yet.</p>
      ) : (
        <table style={table.table}>
          <thead>
            <tr>
              <th style={table.th}>Name</th>
              <th style={table.th}>Category</th>
              <th style={table.th}>Created</th>
              <th style={table.th}>Status</th>
              <th style={table.th}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {tokens.map((token) => (
              <tr key={token.id}>
                <td style={table.td}>{token.name}</td>
                <td style={table.td}>{token.category_name || '\u2014'}</td>
                <td style={table.td}>{new Date(token.created_at).toLocaleString()}</td>
                <td style={table.td}>
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
                <td style={table.td}>
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
