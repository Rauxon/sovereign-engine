import { useTheme } from '../../theme';
import type { ThemeColors } from '../../theme';
import type { ReservationStatus } from '../../types';

interface BadgeStyle {
  label: string;
  bg: (c: ThemeColors) => string;
  text: (c: ThemeColors) => string;
}

const STATUS_CONFIG: Record<ReservationStatus, BadgeStyle> = {
  pending:   { label: 'Pending',   bg: (c) => c.badgeWarningBg, text: (c) => c.badgeWarningText },
  approved:  { label: 'Approved',  bg: (c) => c.badgeInfoBg,    text: (c) => c.badgeInfoText },
  active:    { label: 'Active',    bg: (c) => c.badgeSuccessBg, text: (c) => c.badgeSuccessText },
  completed: { label: 'Completed', bg: (c) => c.badgeNeutralBg, text: (c) => c.badgeNeutralText },
  rejected:  { label: 'Rejected',  bg: (c) => c.badgeDangerBg,  text: (c) => c.badgeDangerText },
  cancelled: { label: 'Cancelled', bg: (c) => c.badgeNeutralBg, text: (c) => c.badgeNeutralText },
};

export default function ReservationStatusBadge({ status }: { status: ReservationStatus }) {
  const { colors } = useTheme();
  const cfg = STATUS_CONFIG[status] ?? STATUS_CONFIG.pending;

  return (
    <span
      style={{
        display: 'inline-block',
        padding: '0.15rem 0.5rem',
        borderRadius: 4,
        fontSize: '0.75rem',
        fontWeight: 600,
        background: cfg.bg(colors),
        color: cfg.text(colors),
      }}
    >
      {cfg.label}
    </span>
  );
}
