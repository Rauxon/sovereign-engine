import { useTheme } from '../../theme';

interface ErrorAlertProps {
  message: string;
  onRetry?: () => void;
}

export default function ErrorAlert({ message, onRetry }: ErrorAlertProps) {
  const { colors } = useTheme();

  return (
    <div style={{
      padding: '1rem 1.25rem',
      background: colors.badgeDangerBg,
      border: `1px solid ${colors.dangerText}`,
      borderRadius: 6,
      color: colors.dangerText,
      marginBottom: '1rem',
      display: 'flex',
      alignItems: 'center',
      gap: '0.75rem',
    }}>
      <span style={{ flex: 1 }}>{message}</span>
      {onRetry && (
        <button
          onClick={onRetry}
          style={{
            padding: '0.4rem 0.8rem',
            background: colors.buttonDanger,
            color: '#fff',
            border: 'none',
            borderRadius: 4,
            cursor: 'pointer',
            fontSize: '0.85rem',
          }}
        >
          Retry
        </button>
      )}
    </div>
  );
}
