import { useState, useEffect, useCallback } from 'react';
import { getAdminUsers, updateUser } from '../../api';
import type { AdminUser } from '../../types';
import { useTheme, tableStyles } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

export default function Users() {
  const { colors } = useTheme();
  const [users, setUsers] = useState<AdminUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [toggling, setToggling] = useState<string | null>(null);
  const [confirmToggle, setConfirmToggle] = useState<AdminUser | null>(null);

  const { table: tableStyle, th: thStyle, td: tdStyle } = tableStyles(colors);

  const fetchUsers = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getAdminUsers();
      setUsers(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load users');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchUsers();
  }, [fetchUsers]);

  const handleToggleAdmin = async (user: AdminUser) => {
    setConfirmToggle(null);
    setToggling(user.id);
    try {
      await updateUser(user.id, { is_admin: !user.is_admin });
      setUsers((prev) =>
        prev.map((u) => (u.id === user.id ? { ...u, is_admin: !u.is_admin } : u))
      );
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to update user');
    } finally {
      setToggling(null);
    }
  };

  if (loading) return <LoadingSpinner message="Loading users..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchUsers} />;

  return (
    <div>
      <h1>Users</h1>

      {users.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No users found.</p>
      ) : (
        <table style={tableStyle}>
          <thead>
            <tr>
              <th style={thStyle}>Email</th>
              <th style={thStyle}>Display Name</th>
              <th style={thStyle}>Admin</th>
              <th style={thStyle}>Requests</th>
              <th style={thStyle}>Total Tokens</th>
              <th style={thStyle}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {users.map((user) => (
              <tr key={user.id}>
                <td style={tdStyle}>{user.email || '\u2014'}</td>
                <td style={tdStyle}>{user.display_name || '\u2014'}</td>
                <td style={tdStyle}>
                  <span
                    style={{
                      display: 'inline-block',
                      padding: '0.2rem 0.6rem',
                      borderRadius: 12,
                      fontSize: '0.8rem',
                      fontWeight: 600,
                      background: user.is_admin ? colors.badgePurpleBg : colors.badgeNeutralBg,
                      color: user.is_admin ? colors.badgePurpleText : colors.badgeNeutralText,
                    }}
                  >
                    {user.is_admin ? 'Admin' : 'User'}
                  </span>
                </td>
                <td style={tdStyle}>{formatNumber(user.usage_summary.total_requests)}</td>
                <td style={tdStyle}>{formatNumber(user.usage_summary.total_tokens)}</td>
                <td style={tdStyle}>
                  <button
                    onClick={() => setConfirmToggle(user)}
                    disabled={toggling === user.id}
                    style={{
                      padding: '0.3rem 0.7rem',
                      background: user.is_admin ? colors.buttonDanger : colors.successText,
                      color: '#fff',
                      border: 'none',
                      borderRadius: 4,
                      cursor: toggling === user.id ? 'default' : 'pointer',
                      fontSize: '0.8rem',
                      opacity: toggling === user.id ? 0.5 : 1,
                    }}
                  >
                    {(() => {
                      if (toggling === user.id) return 'Updating...';
                      return user.is_admin ? 'Remove Admin' : 'Make Admin';
                    })()}
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      {confirmToggle && (
        <ConfirmDialog
          title={confirmToggle.is_admin ? 'Remove Admin' : 'Grant Admin'}
          message={
            confirmToggle.is_admin
              ? `Remove admin privileges from ${confirmToggle.email || confirmToggle.display_name || 'this user'}?`
              : `Grant admin privileges to ${confirmToggle.email || confirmToggle.display_name || 'this user'}?`
          }
          confirmLabel={confirmToggle.is_admin ? 'Remove Admin' : 'Make Admin'}
          destructive={confirmToggle.is_admin}
          onConfirm={() => handleToggleAdmin(confirmToggle)}
          onCancel={() => setConfirmToggle(null)}
        />
      )}
    </div>
  );
}
