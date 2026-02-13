import { useTheme } from '../../theme';
import type { ThemeMode } from '../../theme';

const CYCLE: ThemeMode[] = ['system', 'light', 'dark'];
const ICONS: Record<ThemeMode, string> = { system: '\u{1F5A5}', light: '\u2600', dark: '\u{1F319}' };
const LABELS: Record<ThemeMode, string> = { system: 'System', light: 'Light', dark: 'Dark' };

export default function ThemeToggle() {
  const { mode, setMode } = useTheme();

  const next = () => {
    const idx = CYCLE.indexOf(mode);
    setMode(CYCLE[(idx + 1) % CYCLE.length]);
  };

  return (
    <button
      onClick={next}
      title={`Theme: ${LABELS[mode]}`}
      style={{
        background: 'transparent',
        border: 'none',
        cursor: 'pointer',
        fontSize: '1.1rem',
        padding: '0.2rem 0.4rem',
        lineHeight: 1,
      }}
    >
      {ICONS[mode]}
    </button>
  );
}
