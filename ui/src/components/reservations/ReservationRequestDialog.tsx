import { useState } from 'react';
import { useTheme } from '../../theme';
import { createReservation } from '../../api';

interface ReservationRequestDialogProps {
  startTime: string;
  endTime: string;
  onCreated: () => void;
  onCancel: () => void;
}

/** Convert an ISO string (e.g. "2026-02-15T09:00:00") to datetime-local value ("2026-02-15T09:00") */
function toDatetimeLocal(iso: string): string {
  return iso.slice(0, 16);
}

/** Convert a datetime-local string (local time) to a UTC ISO string (no trailing Z, for the backend). */
function localToUtc(datetimeLocal: string): string {
  return new Date(datetimeLocal).toISOString().slice(0, 19);
}

export default function ReservationRequestDialog({
  startTime,
  endTime,
  onCreated,
  onCancel,
}: ReservationRequestDialogProps) {
  const { colors } = useTheme();
  const [editStart, setEditStart] = useState(() => toDatetimeLocal(startTime));
  const [editEnd, setEditEnd] = useState(() => toDatetimeLocal(endTime));
  const [reason, setReason] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleSubmit = async () => {
    setSubmitting(true);
    setError(null);
    try {
      // Convert local times to UTC for the backend
      await createReservation({
        start_time: localToUtc(editStart),
        end_time: localToUtc(editEnd),
        reason: reason || undefined,
      });
      onCreated();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create reservation');
      setSubmitting(false);
    }
  };

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '0.5rem',
    border: `1px solid ${colors.inputBorder}`,
    borderRadius: 4,
    fontSize: '0.9rem',
    boxSizing: 'border-box',
    background: colors.inputBg,
    color: colors.textPrimary,
    fontFamily: 'inherit',
  };

  return (
    <div
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
      onClick={onCancel}
    >
      <div
        style={{
          background: colors.dialogBg,
          borderRadius: 8,
          padding: '1.5rem',
          maxWidth: 440,
          width: '90%',
          boxShadow: colors.dialogShadow,
        }}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 style={{ margin: '0 0 0.75rem', color: colors.textPrimary }}>Request Reservation</h3>

        <div style={{ marginBottom: '0.75rem' }}>
          <label
            style={{
              display: 'block',
              marginBottom: '0.25rem',
              fontSize: '0.85rem',
              fontWeight: 600,
              color: colors.textSecondary,
            }}
          >
            Start
          </label>
          <input
            type="datetime-local"
            value={editStart}
            onChange={(e) => setEditStart(e.target.value)}
            style={inputStyle}
          />
        </div>

        <div style={{ marginBottom: '0.75rem' }}>
          <label
            style={{
              display: 'block',
              marginBottom: '0.25rem',
              fontSize: '0.85rem',
              fontWeight: 600,
              color: colors.textSecondary,
            }}
          >
            End
          </label>
          <input
            type="datetime-local"
            value={editEnd}
            onChange={(e) => setEditEnd(e.target.value)}
            style={inputStyle}
          />
        </div>

        <div style={{ marginBottom: '0.75rem' }}>
          <label
            style={{
              display: 'block',
              marginBottom: '0.25rem',
              fontSize: '0.85rem',
              fontWeight: 600,
              color: colors.textSecondary,
            }}
          >
            Reason (optional)
          </label>
          <textarea
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            rows={3}
            style={{
              ...inputStyle,
              resize: 'vertical',
            }}
            placeholder="What do you need the system for?"
          />
        </div>

        {error && (
          <div style={{ fontSize: '0.85rem', color: colors.dangerText, marginBottom: '0.75rem', fontWeight: 600 }}>
            {error}
          </div>
        )}

        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.75rem' }}>
          <button
            onClick={onCancel}
            disabled={submitting}
            style={{
              padding: '0.5rem 1rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: submitting ? 'default' : 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={submitting}
            style={{
              padding: '0.5rem 1rem',
              background: submitting ? colors.buttonPrimaryDisabled : colors.buttonPrimary,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: submitting ? 'default' : 'pointer',
              fontWeight: 600,
            }}
          >
            {submitting ? 'Submitting...' : 'Request Booking'}
          </button>
        </div>
      </div>
    </div>
  );
}
