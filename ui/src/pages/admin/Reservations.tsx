import { useState, useEffect, useCallback } from 'react';
import {
  getAdminReservations,
  approveReservation,
  rejectReservation,
  activateReservation,
  deactivateReservation,
  deleteReservation,
  getCalendarReservations,
} from '../../api';
import type { ReservationWithUser, ReservationStatus } from '../../types';
import { useTheme } from '../../theme';
import { useEventStream } from '../../hooks/useEventStream';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';
import WeekCalendar from '../../components/reservations/WeekCalendar';
import ReservationStatusBadge from '../../components/reservations/ReservationStatusBadge';

function formatDateTime(iso: string): string {
  return new Date(iso + 'Z').toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

type StatusFilter = 'all' | ReservationStatus;

export default function AdminReservations({ userId }: Readonly<{ userId: string }>) {
  const { colors } = useTheme();
  const { reservationRevision: revision } = useEventStream();

  const [reservations, setReservations] = useState<ReservationWithUser[]>([]);
  const [calendarReservations, setCalendarReservations] = useState<ReservationWithUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [statusFilter, setStatusFilter] = useState<StatusFilter>('all');

  // Admin note input for approve/reject
  const [noteInput, setNoteInput] = useState('');
  const [pendingAction, setPendingAction] = useState<{ id: string; action: 'approve' | 'reject' } | null>(null);
  const [confirmDeactivate, setConfirmDeactivate] = useState<string | null>(null);
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);
  const [acting, setActing] = useState(false);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [all, cal] = await Promise.all([getAdminReservations(), getCalendarReservations()]);
      setReservations(all);
      setCalendarReservations(cal);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load reservations');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData, revision]);

  const handleApproveReject = async () => {
    if (!pendingAction) return;
    setActing(true);
    setActionError(null);
    try {
      if (pendingAction.action === 'approve') {
        await approveReservation(pendingAction.id, noteInput || undefined);
      } else {
        await rejectReservation(pendingAction.id, noteInput || undefined);
      }
      setPendingAction(null);
      setNoteInput('');
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Action failed');
    } finally {
      setActing(false);
    }
  };

  const handleActivate = async (id: string) => {
    setActionError(null);
    try {
      await activateReservation(id);
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to activate');
    }
  };

  const handleDeactivate = async (id: string) => {
    setActing(true);
    setActionError(null);
    try {
      await deactivateReservation(id);
      setConfirmDeactivate(null);
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to deactivate');
    } finally {
      setActing(false);
    }
  };

  const handleDelete = async (id: string) => {
    setActing(true);
    setActionError(null);
    try {
      await deleteReservation(id);
      setConfirmDelete(null);
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to delete');
    } finally {
      setActing(false);
    }
  };

  const filtered = statusFilter === 'all' ? reservations : reservations.filter((r) => r.status === statusFilter);
  const pending = reservations.filter((r) => r.status === 'pending');
  const activeRes = reservations.find((r) => r.status === 'active');

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

  const btnSmall = (bg: string, disabled = false): React.CSSProperties => ({
    padding: '0.2rem 0.5rem',
    background: disabled ? colors.buttonPrimaryDisabled : bg,
    color: '#fff',
    border: 'none',
    borderRadius: 4,
    cursor: disabled ? 'default' : 'pointer',
    fontSize: '0.75rem',
    marginRight: '0.25rem',
  });

  if (loading) return <LoadingSpinner message="Loading reservations..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchData} />;

  return (
    <div>
      <h2 style={{ margin: '0 0 1rem', color: colors.textPrimary }}>Reservations (Admin)</h2>

      {actionError && (
        <div style={{ marginBottom: '1rem' }}>
          <ErrorAlert message={actionError} />
        </div>
      )}

      {/* Active reservation card */}
      {activeRes && (
        <div
          style={{
            background: colors.badgeWarningBg,
            border: `1px solid ${colors.warningText}`,
            borderRadius: 8,
            padding: '1rem',
            marginBottom: '1.5rem',
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: '0.5rem' }}>
            <div>
              <strong style={{ color: colors.warningText }}>Active Reservation</strong>
              <div style={{ fontSize: '0.85rem', color: colors.textSecondary, marginTop: '0.25rem' }}>
                {activeRes.user_display_name || activeRes.user_email || activeRes.user_id} — until {formatDateTime(activeRes.end_time)}
              </div>
              {activeRes.reason && (
                <div style={{ fontSize: '0.85rem', color: colors.textMuted, marginTop: '0.25rem' }}>
                  Reason: {activeRes.reason}
                </div>
              )}
            </div>
            <button
              onClick={() => setConfirmDeactivate(activeRes.id)}
              style={{
                padding: '0.4rem 0.8rem',
                background: colors.buttonDanger,
                color: '#fff',
                border: 'none',
                borderRadius: 4,
                cursor: 'pointer',
                fontSize: '0.85rem',
                fontWeight: 600,
              }}
            >
              End Early
            </button>
          </div>
        </div>
      )}

      {/* Calendar */}
      <div
        style={{
          background: colors.cardBg,
          border: `1px solid ${colors.cardBorder}`,
          borderRadius: 8,
          padding: '1rem',
          marginBottom: '1.5rem',
        }}
      >
        <h3 style={{ margin: '0 0 0.75rem', color: colors.textPrimary, fontSize: '1rem' }}>Schedule</h3>
        <WeekCalendar reservations={calendarReservations} currentUserId={userId} />
      </div>

      {/* Pending requests */}
      {pending.length > 0 && (
        <div style={{ marginBottom: '1.5rem' }}>
          <h3 style={{ margin: '0 0 0.75rem', color: colors.textPrimary, fontSize: '1rem' }}>
            Pending Requests ({pending.length})
          </h3>
          <div style={{ display: 'flex', flexDirection: 'column', gap: '0.5rem' }}>
            {pending.map((r) => (
              <div
                key={r.id}
                style={{
                  background: colors.cardBg,
                  border: `1px solid ${colors.cardBorder}`,
                  borderRadius: 8,
                  padding: '0.75rem 1rem',
                  display: 'flex',
                  justifyContent: 'space-between',
                  alignItems: 'center',
                  flexWrap: 'wrap',
                  gap: '0.5rem',
                }}
              >
                <div>
                  <strong style={{ color: colors.textPrimary, fontSize: '0.9rem' }}>
                    {r.user_display_name || r.user_email || r.user_id}
                  </strong>
                  <div style={{ fontSize: '0.85rem', color: colors.textSecondary }}>
                    {formatDateTime(r.start_time)} — {formatDateTime(r.end_time)}
                  </div>
                  {r.reason && (
                    <div style={{ fontSize: '0.8rem', color: colors.textMuted }}>{r.reason}</div>
                  )}
                </div>
                <div style={{ display: 'flex', gap: '0.35rem' }}>
                  <button
                    onClick={() => { setPendingAction({ id: r.id, action: 'approve' }); setNoteInput(''); }}
                    style={btnSmall(colors.successText)}
                  >
                    Approve
                  </button>
                  <button
                    onClick={() => { setPendingAction({ id: r.id, action: 'reject' }); setNoteInput(''); }}
                    style={btnSmall(colors.buttonDanger)}
                  >
                    Reject
                  </button>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* All reservations table with filter */}
      <div style={{ display: 'flex', alignItems: 'center', gap: '0.75rem', marginBottom: '0.75rem' }}>
        <h3 style={{ margin: 0, color: colors.textPrimary, fontSize: '1rem' }}>All Reservations</h3>
        <select
          value={statusFilter}
          onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
          style={{
            padding: '0.3rem 0.5rem',
            border: `1px solid ${colors.inputBorder}`,
            borderRadius: 4,
            fontSize: '0.8rem',
            background: colors.inputBg,
            color: colors.textPrimary,
          }}
        >
          <option value="all">All</option>
          <option value="pending">Pending</option>
          <option value="approved">Approved</option>
          <option value="active">Active</option>
          <option value="completed">Completed</option>
          <option value="rejected">Rejected</option>
          <option value="cancelled">Cancelled</option>
        </select>
      </div>

      {filtered.length === 0 ? (
        <p style={{ color: colors.textMuted }}>No reservations found.</p>
      ) : (
        <div style={{ overflowX: 'auto' }}>
          <table style={tableStyle}>
            <thead>
              <tr>
                <th style={thStyle}>Status</th>
                <th style={thStyle}>User</th>
                <th style={thStyle}>Start</th>
                <th style={thStyle}>End</th>
                <th style={thStyle}>Reason</th>
                <th style={thStyle}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {filtered.map((r) => (
                <tr key={r.id}>
                  <td style={tdStyle}><ReservationStatusBadge status={r.status} /></td>
                  <td style={tdStyle}>{r.user_display_name || r.user_email || r.user_id}</td>
                  <td style={tdStyle}>{formatDateTime(r.start_time)}</td>
                  <td style={tdStyle}>{formatDateTime(r.end_time)}</td>
                  <td style={{ ...tdStyle, maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                    {r.reason || '-'}
                  </td>
                  <td style={tdStyle}>
                    {r.status === 'pending' && (
                      <>
                        <button onClick={() => { setPendingAction({ id: r.id, action: 'approve' }); setNoteInput(''); }} style={btnSmall(colors.successText)}>Approve</button>
                        <button onClick={() => { setPendingAction({ id: r.id, action: 'reject' }); setNoteInput(''); }} style={btnSmall(colors.buttonDanger)}>Reject</button>
                      </>
                    )}
                    {r.status === 'approved' && (
                      <button onClick={() => handleActivate(r.id)} style={btnSmall(colors.buttonPrimary)}>Activate Now</button>
                    )}
                    {r.status === 'active' && (
                      <button onClick={() => setConfirmDeactivate(r.id)} style={btnSmall(colors.buttonDanger)}>End</button>
                    )}
                    {r.status !== 'active' && (
                      <button onClick={() => setConfirmDelete(r.id)} style={btnSmall(colors.buttonDisabled)}>Del</button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* Approve/Reject dialog with note */}
      {pendingAction && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={`${pendingAction.action === 'approve' ? 'Approve' : 'Reject'} Reservation`}
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            background: colors.overlayBg,
            display: 'flex',
            justifyContent: 'center',
            alignItems: 'center',
            zIndex: 1000,
          }}
          onClick={() => setPendingAction(null)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              setPendingAction(null);
            }
          }}
        >
          <div
            style={{
              background: colors.dialogBg,
              borderRadius: 8,
              padding: '1.5rem',
              maxWidth: 420,
              width: '90%',
              boxShadow: colors.dialogShadow,
            }}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            <h3 style={{ margin: '0 0 0.75rem', color: colors.textPrimary }}>
              {pendingAction.action === 'approve' ? 'Approve' : 'Reject'} Reservation
            </h3>
            <div style={{ marginBottom: '0.75rem' }}>
              <label htmlFor="admin-reservation-note" style={{ display: 'block', marginBottom: '0.25rem', fontSize: '0.85rem', fontWeight: 600, color: colors.textSecondary }}>
                Note (optional)
              </label>
              <textarea
                id="admin-reservation-note"
                value={noteInput}
                onChange={(e) => setNoteInput(e.target.value)}
                rows={2}
                style={{
                  width: '100%',
                  padding: '0.5rem',
                  border: `1px solid ${colors.inputBorder}`,
                  borderRadius: 4,
                  fontSize: '0.9rem',
                  boxSizing: 'border-box',
                  background: colors.inputBg,
                  color: colors.textPrimary,
                  resize: 'vertical',
                  fontFamily: 'inherit',
                }}
              />
            </div>
            <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.75rem' }}>
              <button
                onClick={() => setPendingAction(null)}
                disabled={acting}
                style={{
                  padding: '0.5rem 1rem',
                  background: colors.buttonDisabled,
                  color: colors.textSecondary,
                  border: 'none',
                  borderRadius: 4,
                  cursor: acting ? 'default' : 'pointer',
                }}
              >
                Cancel
              </button>
              {(() => {
                const isApprove = pendingAction.action === 'approve';
                let actionBg: string;
                let actionLabel: string;
                if (acting) {
                  actionBg = colors.buttonPrimaryDisabled;
                  actionLabel = 'Processing...';
                } else if (isApprove) {
                  actionBg = colors.successText;
                  actionLabel = 'Approve';
                } else {
                  actionBg = colors.buttonDanger;
                  actionLabel = 'Reject';
                }
                return (
                  <button
                    onClick={handleApproveReject}
                    disabled={acting}
                    style={{
                      padding: '0.5rem 1rem',
                      background: actionBg,
                      color: '#fff',
                      border: 'none',
                      borderRadius: 4,
                      cursor: acting ? 'default' : 'pointer',
                      fontWeight: 600,
                    }}
                  >
                    {actionLabel}
                  </button>
                );
              })()}
            </div>
          </div>
        </div>
      )}

      {/* Confirm deactivate */}
      {confirmDeactivate && (
        <ConfirmDialog
          title="End Reservation Early"
          message="This will immediately end the active reservation and open the system for all users."
          confirmLabel={acting ? 'Ending...' : 'End Reservation'}
          onConfirm={() => handleDeactivate(confirmDeactivate)}
          onCancel={() => setConfirmDeactivate(null)}
          destructive
        />
      )}

      {/* Confirm delete */}
      {confirmDelete && (
        <ConfirmDialog
          title="Delete Reservation"
          message="This will permanently delete this reservation record."
          confirmLabel={acting ? 'Deleting...' : 'Delete'}
          onConfirm={() => handleDelete(confirmDelete)}
          onCancel={() => setConfirmDelete(null)}
          destructive
        />
      )}
    </div>
  );
}
