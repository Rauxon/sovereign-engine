import { useRef, useEffect } from 'react';
import { useTheme } from '../../theme';

type ConfirmDialogProps = Readonly<{
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void;
  onCancel: () => void;
  destructive?: boolean;
}>

export default function ConfirmDialog({
  title,
  message,
  confirmLabel = 'Confirm',
  onConfirm,
  onCancel,
  destructive = false,
}: ConfirmDialogProps) {
  const { colors } = useTheme();
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    dialogRef.current?.showModal();
  }, []);

  const handleClose = () => {
    onCancel();
  };

  return (
    <>
      <style>{`.confirm-dialog::backdrop { background: ${colors.overlayBg}; }`}</style>
      <dialog
        ref={dialogRef}
        className="confirm-dialog"
        style={{
          border: 'none',
          borderRadius: 8,
          padding: '1.5rem',
          maxWidth: 420,
          width: '90%',
          boxShadow: colors.dialogShadow,
          background: colors.dialogBg,
          color: 'inherit',
        }}
        onClose={handleClose}
        onClick={(e) => {
          if (e.target === e.currentTarget) onCancel();
        }}
      >
        <h3 style={{ margin: '0 0 0.75rem', color: colors.textPrimary }}>{title}</h3>
        <p style={{ margin: 0, color: colors.textMuted, lineHeight: 1.5 }}>{message}</p>
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.75rem', marginTop: '1.25rem' }}>
          <button
            onClick={onCancel}
            style={{
              padding: '0.5rem 1rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            onClick={onConfirm}
            style={{
              padding: '0.5rem 1rem',
              background: destructive ? colors.buttonDanger : colors.buttonPrimary,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
            }}
          >
            {confirmLabel}
          </button>
        </div>
      </dialog>
    </>
  );
}
