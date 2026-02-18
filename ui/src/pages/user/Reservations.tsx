import { useState, useEffect, useCallback } from 'react';
import {
  getUserReservations,
  getCalendarReservations,
  cancelReservation,
  getActiveReservation,
  getUserModels,
  reservationStartContainer,
  reservationStopContainer,
  getSystemInfo,
} from '../../api';
import type { Reservation, ReservationWithUser, AdminModel } from '../../types';
import { useTheme } from '../../theme';
import { useEventStream } from '../../hooks/useEventStream';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import ConfirmDialog from '../../components/common/ConfirmDialog';
import WeekCalendar from '../../components/reservations/WeekCalendar';
import ReservationStatusBadge from '../../components/reservations/ReservationStatusBadge';
import ReservationRequestDialog from '../../components/reservations/ReservationRequestDialog';

function formatDateTime(iso: string): string {
  return new Date(iso + 'Z').toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export default function Reservations({ userId }: Readonly<{ userId: string }>) {
  const { colors } = useTheme();
  const { reservationRevision: revision } = useEventStream();

  const [reservations, setReservations] = useState<Reservation[]>([]);
  const [calendarReservations, setCalendarReservations] = useState<ReservationWithUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Active reservation state
  const [isActiveHolder, setIsActiveHolder] = useState(false);
  const [models, setModels] = useState<AdminModel[]>([]);
  const [availableBackends, setAvailableBackends] = useState<string[]>([]);

  // Dialog state
  const [showMyReservations, setShowMyReservations] = useState(false);
  const [slotSelection, setSlotSelection] = useState<{ start: string; end: string } | null>(null);
  const [confirmCancel, setConfirmCancel] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);

  // Container management for reservation holder
  const [startingModel, setStartingModel] = useState<string | null>(null);
  const [stoppingModel, setStoppingModel] = useState<string | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [own, cal, active] = await Promise.all([
        getUserReservations(),
        getCalendarReservations(),
        getActiveReservation(),
      ]);
      setReservations(own);
      setCalendarReservations(cal);
      setIsActiveHolder(active.active && active.user_id === userId);

      if (active.active && active.user_id === userId) {
        const [m, sys] = await Promise.all([getUserModels(), getSystemInfo()]);
        setModels(m);
        setAvailableBackends(sys.available_backends || []);
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load reservations');
    } finally {
      setLoading(false);
    }
  }, [userId]);

  useEffect(() => {
    fetchData();
  }, [fetchData, revision]);

  const handleCancel = async (id: string) => {
    setCancelling(true);
    setActionError(null);
    try {
      await cancelReservation(id);
      setConfirmCancel(null);
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to cancel');
    } finally {
      setCancelling(false);
    }
  };

  const handleStartContainer = async (modelId: string) => {
    setStartingModel(modelId);
    setActionError(null);
    try {
      const gpu = availableBackends.find((b) => b !== 'none') || 'none';
      await reservationStartContainer({ model_id: modelId, gpu_type: gpu });
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to start container');
    } finally {
      setStartingModel(null);
    }
  };

  const handleStopContainer = async (modelId: string) => {
    setStoppingModel(modelId);
    setActionError(null);
    try {
      await reservationStopContainer(modelId);
      fetchData();
    } catch (err) {
      setActionError(err instanceof Error ? err.message : 'Failed to stop container');
    } finally {
      setStoppingModel(null);
    }
  };

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

  if (loading) return <LoadingSpinner message="Loading reservations..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchData} />;

  return (
    <div>
      <h2 style={{ margin: '0 0 1rem', color: colors.textPrimary }}>Reservations</h2>

      {actionError && (
        <div style={{ marginBottom: '1rem' }}>
          <ErrorAlert message={actionError} />
        </div>
      )}

      {/* Active reservation: container controls */}
      {isActiveHolder && (
        <div
          style={{
            background: colors.badgeSuccessBg,
            border: `1px solid ${colors.successText}`,
            borderRadius: 8,
            padding: '1rem',
            marginBottom: '1.5rem',
          }}
        >
          <h3 style={{ margin: '0 0 0.75rem', color: colors.successText, fontSize: '1rem' }}>
            You hold the active reservation
          </h3>
          <p style={{ margin: '0 0 0.75rem', fontSize: '0.85rem', color: colors.textSecondary }}>
            You can start and stop model containers during your reservation.
          </p>

          <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap' }}>
            {models.map((m) => (
              <div
                key={m.id}
                style={{
                  background: colors.cardBg,
                  border: `1px solid ${colors.cardBorder}`,
                  borderRadius: 6,
                  padding: '0.5rem 0.75rem',
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.5rem',
                  fontSize: '0.85rem',
                }}
              >
                <span style={{ color: colors.textPrimary }}>{m.hf_repo}</span>
                {m.loaded ? (
                  <button
                    onClick={() => handleStopContainer(m.id)}
                    disabled={stoppingModel === m.id}
                    style={{
                      padding: '0.2rem 0.5rem',
                      background: colors.buttonDanger,
                      color: '#fff',
                      border: 'none',
                      borderRadius: 4,
                      cursor: stoppingModel === m.id ? 'default' : 'pointer',
                      fontSize: '0.75rem',
                    }}
                  >
                    {stoppingModel === m.id ? 'Stopping...' : 'Stop'}
                  </button>
                ) : (
                  <button
                    onClick={() => handleStartContainer(m.id)}
                    disabled={startingModel === m.id}
                    style={{
                      padding: '0.2rem 0.5rem',
                      background: startingModel === m.id ? colors.buttonPrimaryDisabled : colors.successText,
                      color: '#fff',
                      border: 'none',
                      borderRadius: 4,
                      cursor: startingModel === m.id ? 'default' : 'pointer',
                      fontSize: '0.75rem',
                    }}
                  >
                    {startingModel === m.id ? 'Starting...' : 'Start'}
                  </button>
                )}
              </div>
            ))}
            {models.length === 0 && (
              <span style={{ color: colors.textMuted, fontSize: '0.85rem' }}>No models registered</span>
            )}
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
        <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', margin: '0 0 0.75rem' }}>
          <h3 style={{ margin: 0, color: colors.textPrimary, fontSize: '1rem' }}>
            Schedule
          </h3>
          <button
            onClick={() => setShowMyReservations(true)}
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
            My Reservations ({reservations.length})
          </button>
        </div>
        <WeekCalendar
          reservations={calendarReservations}
          currentUserId={userId}
          onSlotSelect={(start, end) => setSlotSelection({ start, end })}
        />
      </div>

      {/* Dialogs */}
      {slotSelection && (
        <ReservationRequestDialog
          startTime={slotSelection.start}
          endTime={slotSelection.end}
          onCreated={() => {
            setSlotSelection(null);
            fetchData();
          }}
          onCancel={() => setSlotSelection(null)}
        />
      )}

      {confirmCancel && (
        <ConfirmDialog
          title="Cancel Reservation"
          message="Are you sure you want to cancel this reservation?"
          confirmLabel={cancelling ? 'Cancelling...' : 'Cancel Reservation'}
          onConfirm={() => handleCancel(confirmCancel)}
          onCancel={() => setConfirmCancel(null)}
          destructive
        />
      )}

      {/* My Reservations modal */}
      {showMyReservations && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label="My Reservations"
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
          onClick={() => setShowMyReservations(false)}
          onKeyDown={(e) => {
            if (e.key === 'Escape') {
              e.preventDefault();
              setShowMyReservations(false);
            }
          }}
        >
          <div
            style={{
              background: colors.dialogBg,
              borderRadius: 8,
              padding: '1.5rem',
              maxWidth: 700,
              width: '90%',
              maxHeight: '80vh',
              overflowY: 'auto',
              boxShadow: colors.dialogShadow,
            }}
            onClick={(e) => e.stopPropagation()}
            onKeyDown={(e) => e.stopPropagation()}
          >
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', margin: '0 0 1rem' }}>
              <h3 style={{ margin: 0, color: colors.textPrimary }}>My Reservations</h3>
              <button
                onClick={() => setShowMyReservations(false)}
                style={{
                  background: 'none',
                  border: 'none',
                  color: colors.textMuted,
                  fontSize: '1.2rem',
                  cursor: 'pointer',
                  padding: '0.25rem 0.5rem',
                  lineHeight: 1,
                }}
              >
                ✕
              </button>
            </div>
            {reservations.length === 0 ? (
              <p style={{ color: colors.textMuted }}>No reservations yet. Click on the calendar to book a slot.</p>
            ) : (
              <div style={{ overflowX: 'auto' }}>
                <table style={tableStyle}>
                  <thead>
                    <tr>
                      <th style={thStyle}>Status</th>
                      <th style={thStyle}>Start</th>
                      <th style={thStyle}>End</th>
                      <th style={thStyle}>Reason</th>
                      <th style={thStyle}>Admin Note</th>
                      <th style={thStyle}>Actions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {reservations.map((r) => (
                      <tr key={r.id}>
                        <td style={tdStyle}><ReservationStatusBadge status={r.status} /></td>
                        <td style={tdStyle}>{formatDateTime(r.start_time)}</td>
                        <td style={tdStyle}>{formatDateTime(r.end_time)}</td>
                        <td style={{ ...tdStyle, maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {r.reason || '-'}
                        </td>
                        <td style={{ ...tdStyle, maxWidth: 200, overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>
                          {r.admin_note || '-'}
                        </td>
                        <td style={tdStyle}>
                          {(r.status === 'pending' || r.status === 'approved') && (
                            <button
                              onClick={() => setConfirmCancel(r.id)}
                              style={{
                                padding: '0.2rem 0.5rem',
                                background: colors.buttonDanger,
                                color: '#fff',
                                border: 'none',
                                borderRadius: 4,
                                cursor: 'pointer',
                                fontSize: '0.8rem',
                              }}
                            >
                              Cancel
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
