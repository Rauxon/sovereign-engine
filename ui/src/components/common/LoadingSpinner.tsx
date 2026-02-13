import { useTheme } from '../../theme';

export default function LoadingSpinner({ message = 'Loading...' }: { message?: string }) {
  const { colors } = useTheme();

  const spinnerStyle: React.CSSProperties = {
    display: 'flex',
    justifyContent: 'center',
    alignItems: 'center',
    padding: '2rem',
    color: colors.spinnerColor,
    fontSize: '1rem',
  };

  const dotStyle: React.CSSProperties = {
    display: 'inline-block',
    width: 8,
    height: 8,
    borderRadius: '50%',
    backgroundColor: colors.spinnerColor,
    margin: '0 3px',
    animation: 'pulse 1.4s infinite ease-in-out both',
  };

  return (
    <div style={spinnerStyle}>
      <style>{`
        @keyframes pulse {
          0%, 80%, 100% { transform: scale(0.6); opacity: 0.5; }
          40% { transform: scale(1); opacity: 1; }
        }
      `}</style>
      <span style={{ ...dotStyle, animationDelay: '-0.32s' }} />
      <span style={{ ...dotStyle, animationDelay: '-0.16s' }} />
      <span style={dotStyle} />
      <span style={{ marginLeft: '0.75rem' }}>{message}</span>
    </div>
  );
}
