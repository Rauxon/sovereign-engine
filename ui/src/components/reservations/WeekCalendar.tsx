import { useState, useMemo, useCallback, useRef, useEffect } from 'react';
import { useTheme } from '../../theme';
import type { ReservationWithUser } from '../../types';

type WeekCalendarProps = Readonly<{
  reservations: ReservationWithUser[];
  currentUserId: string;
  onSlotSelect?: (start: string, end: string) => void;
  onReservationClick?: (reservation: ReservationWithUser) => void;
  weekStart?: Date;
}>

// 48 half-hour slots per day (0 = 00:00, 47 = 23:30)
const TOTAL_SLOTS = 48;
// Working hours: slots 14–43 → 07:00–22:00
const WORKING_START = 14;
const WORKING_END = 44; // exclusive — slot 43 is 21:30, ends at 22:00
const CELL_HEIGHT = 28;
const TIME_COL_WIDTH = 52;

function startOfWeek(date: Date): Date {
  const d = new Date(date);
  const day = d.getDay();
  const diff = day === 0 ? -6 : 1 - day; // Monday = start
  d.setDate(d.getDate() + diff);
  d.setHours(0, 0, 0, 0);
  return d;
}

function addDays(date: Date, n: number): Date {
  const d = new Date(date);
  d.setDate(d.getDate() + n);
  return d;
}

function formatTime(slot: number): string {
  const h = Math.floor(slot / 2);
  const m = slot % 2 === 0 ? '00' : '30';
  return `${String(h).padStart(2, '0')}:${m}`;
}

function formatDate(date: Date): string {
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  const days = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
  return `${days[date.getDay()]} ${months[date.getMonth()]} ${date.getDate()}`;
}

function toLocalIso(date: Date, slot: number): string {
  const h = Math.floor(slot / 2);
  const m = slot % 2 === 0 ? 0 : 30;
  const y = date.getFullYear();
  const mo = String(date.getMonth() + 1).padStart(2, '0');
  const d = String(date.getDate()).padStart(2, '0');
  return `${y}-${mo}-${d}T${String(h).padStart(2, '0')}:${String(m).padStart(2, '0')}:00`;
}

/** Get user display label for a reservation block */
function getBlockLabel(r: ReservationWithUser): string {
  if (r.user_display_name) return r.user_display_name;
  if (r.user_email) return r.user_email.split('@')[0];
  return r.user_id.slice(0, 8);
}

/** Get short initials for small blocks */
function getInitials(r: ReservationWithUser): string {
  if (r.user_display_name) {
    return r.user_display_name
      .split(/\s+/)
      .map((w) => w[0])
      .join('')
      .toUpperCase()
      .slice(0, 2);
  }
  if (r.user_email) return r.user_email.slice(0, 2).toUpperCase();
  return r.user_id.slice(0, 2).toUpperCase();
}

interface BlockInfo {
  reservation: ReservationWithUser;
  topSlot: number; // first visible slot
  bottomSlot: number; // last visible slot (inclusive)
  startsBeforeView: boolean;
  endsAfterView: boolean;
}

export default function WeekCalendar({
  reservations,
  currentUserId,
  onSlotSelect,
  onReservationClick,
  weekStart: initialWeekStart,
}: WeekCalendarProps) {
  const { colors } = useTheme();
  const [weekStart, setWeekStart] = useState(() =>
    initialWeekStart ? startOfWeek(initialWeekStart) : startOfWeek(new Date())
  );
  const [showFullDay, setShowFullDay] = useState(false);

  // Selection state for slot picking
  const [selectStart, setSelectStart] = useState<{ day: number; slot: number } | null>(null);
  const [selectEnd, setSelectEnd] = useState<{ day: number; slot: number } | null>(null);

  // Hover state
  const [hoveredCell, setHoveredCell] = useState<{ day: number; slot: number } | null>(null);

  // Now-line
  const [now, setNow] = useState(new Date());
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 60_000);
    return () => clearInterval(timer);
  }, []);

  // Drag state — refs to avoid re-renders during mousemove
  const isDragging = useRef(false);
  const dragStart = useRef<{ day: number; slot: number } | null>(null);

  const visibleStart = showFullDay ? 0 : WORKING_START;
  const visibleEnd = showFullDay ? TOTAL_SLOTS : WORKING_END;
  const visibleSlots = visibleEnd - visibleStart;

  const days = useMemo(() => Array.from({ length: 7 }, (_, i) => addDays(weekStart, i)), [weekStart]);

  const todayStr = now.toDateString();

  // Compute per-day reservation blocks clamped to visible range
  const dayBlocks = useMemo(() => {
    const result: BlockInfo[][] = Array.from({ length: 7 }, () => []);

    for (const r of reservations) {
      const rStart = new Date(r.start_time + 'Z');
      const rEnd = new Date(r.end_time + 'Z');

      for (let dayIdx = 0; dayIdx < 7; dayIdx++) {
        const dayDate = days[dayIdx];
        const dayStart = new Date(dayDate);
        dayStart.setHours(0, 0, 0, 0);
        const dayEnd = new Date(dayDate);
        dayEnd.setHours(24, 0, 0, 0);

        // Does this reservation overlap this day?
        if (rStart >= dayEnd || rEnd <= dayStart) continue;

        // Convert to slot positions within this day
        const effectiveStart = new Date(Math.max(rStart.getTime(), dayStart.getTime()));
        const effectiveEnd = new Date(Math.min(rEnd.getTime(), dayEnd.getTime()));

        const startSlot = effectiveStart.getHours() * 2 + (effectiveStart.getMinutes() >= 30 ? 1 : 0);
        // End slot: the last slot that's occupied (end is exclusive in time)
        const endMinutes = effectiveEnd.getHours() * 60 + effectiveEnd.getMinutes();
        const endSlotExclusive = Math.ceil(endMinutes / 30);
        const lastSlot = endSlotExclusive - 1;

        if (lastSlot < visibleStart || startSlot >= visibleEnd) continue;

        result[dayIdx].push({
          reservation: r,
          topSlot: Math.max(startSlot, visibleStart),
          bottomSlot: Math.min(lastSlot, visibleEnd - 1),
          startsBeforeView: startSlot < visibleStart,
          endsAfterView: lastSlot >= visibleEnd,
        });
      }
    }

    return result;
  }, [reservations, days, visibleStart, visibleEnd]);

  const getReservationColor = (r: ReservationWithUser): string => {
    if (r.status === 'active') return '#f97316'; // orange
    if (r.status === 'pending') return r.user_id === currentUserId ? '#9ca3af' : '#d1d5db';
    if (r.user_id === currentUserId) return '#3b82f6'; // blue
    return '#22c55e'; // green for others' approved
  };

  const isSelected = useCallback(
    (dayIndex: number, slot: number): boolean => {
      if (!selectStart || !selectEnd) return false;
      if (dayIndex !== selectStart.day) return false;
      const minSlot = Math.min(selectStart.slot, selectEnd.slot);
      const maxSlot = Math.max(selectStart.slot, selectEnd.slot);
      return slot >= minSlot && slot <= maxSlot;
    },
    [selectStart, selectEnd]
  );

  // Check if a slot is occupied by a reservation (for interaction layer)
  const isSlotOccupied = useCallback(
    (dayIndex: number, slot: number): ReservationWithUser | null => {
      const blocks = dayBlocks[dayIndex];
      for (const b of blocks) {
        if (slot >= b.topSlot && slot <= b.bottomSlot) return b.reservation;
      }
      return null;
    },
    [dayBlocks]
  );

  // Drag-to-select handlers
  const handleMouseDown = (dayIndex: number, slot: number) => {
    if (!onSlotSelect) return;

    const cellRes = isSlotOccupied(dayIndex, slot);
    if (cellRes) {
      if (onReservationClick) onReservationClick(cellRes);
      return;
    }

    isDragging.current = true;
    dragStart.current = { day: dayIndex, slot };
    setSelectStart({ day: dayIndex, slot });
    setSelectEnd({ day: dayIndex, slot });
  };

  const handleMouseMove = (dayIndex: number, slot: number) => {
    if (!isDragging.current || !dragStart.current) return;
    if (dayIndex !== dragStart.current.day) return;
    setSelectEnd({ day: dayIndex, slot });
  };

  // Global mouseup to finalize drag
  useEffect(() => {
    const handleMouseUp = () => {
      if (!isDragging.current || !dragStart.current) return;
      isDragging.current = false;

      setSelectStart((currentStart) => {
        setSelectEnd((currentEnd) => {
          if (currentStart && currentEnd && onSlotSelect) {
            const startSlot = Math.min(currentStart.slot, currentEnd.slot);
            const endSlot = Math.max(currentStart.slot, currentEnd.slot) + 1;
            const startIso = toLocalIso(days[currentStart.day], startSlot);
            const endIso = toLocalIso(days[currentStart.day], endSlot);
            onSlotSelect(startIso, endIso);
          }
          return currentEnd;
        });
        return currentStart;
      });

      dragStart.current = null;
    };

    globalThis.addEventListener('mouseup', handleMouseUp);
    return () => globalThis.removeEventListener('mouseup', handleMouseUp);
  }, [days, onSlotSelect]);

  const prevWeek = () => setWeekStart((w) => addDays(w, -7));
  const nextWeek = () => setWeekStart((w) => addDays(w, 7));
  const thisWeek = () => setWeekStart(startOfWeek(new Date()));

  // Now-line position (fractional slot within visible range)
  const nowSlotFraction = useMemo(() => {
    const nowDate = now;
    const todayIdx = days.findIndex((d) => d.toDateString() === nowDate.toDateString());
    if (todayIdx === -1) return null;
    const minuteOfDay = nowDate.getHours() * 60 + nowDate.getMinutes();
    const slotFraction = minuteOfDay / 30;
    if (slotFraction < visibleStart || slotFraction > visibleEnd) return null;
    return { dayIdx: todayIdx, offset: (slotFraction - visibleStart) * CELL_HEIGHT };
  }, [now, days, visibleStart, visibleEnd]);

  const gridHeight = visibleSlots * CELL_HEIGHT;

  // Generate hour lines (every 2 hours)
  const hourLines = useMemo(() => {
    const lines: number[] = [];
    for (let slot = visibleStart; slot < visibleEnd; slot++) {
      if (slot % 4 === 0) lines.push(slot);
    }
    return lines;
  }, [visibleStart, visibleEnd]);

  return (
    <div style={{ overflowX: 'auto' }}>
      {/* Navigation bar with inline legend */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: '0.75rem',
          flexWrap: 'wrap',
          gap: '0.5rem',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
          <button
            onClick={prevWeek}
            style={{
              padding: '0.3rem 0.6rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: '0.85rem',
            }}
          >
            Prev
          </button>
          <button
            onClick={thisWeek}
            style={{
              padding: '0.3rem 0.6rem',
              background: colors.buttonPrimary,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: '0.85rem',
            }}
          >
            This Week
          </button>
          <button
            onClick={nextWeek}
            style={{
              padding: '0.3rem 0.6rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: 'pointer',
              fontSize: '0.85rem',
            }}
          >
            Next
          </button>
          <div style={{ display: 'flex', borderRadius: 4, overflow: 'hidden', border: `1px solid ${colors.cardBorder}` }}>
            <button
              onClick={() => setShowFullDay(false)}
              style={{
                padding: '0.3rem 0.6rem',
                background: showFullDay ? 'transparent' : colors.buttonPrimary,
                color: showFullDay ? colors.textMuted : '#fff',
                border: 'none',
                cursor: 'pointer',
                fontSize: '0.75rem',
              }}
            >
              07:00–22:00
            </button>
            <button
              onClick={() => setShowFullDay(true)}
              style={{
                padding: '0.3rem 0.6rem',
                background: showFullDay ? colors.buttonPrimary : 'transparent',
                color: showFullDay ? '#fff' : colors.textMuted,
                border: 'none',
                borderLeft: `1px solid ${colors.cardBorder}`,
                cursor: 'pointer',
                fontSize: '0.75rem',
              }}
            >
              24h
            </button>
          </div>
        </div>

        {/* Inline legend */}
        <div
          style={{
            display: 'flex',
            gap: '0.75rem',
            fontSize: '0.7rem',
            color: colors.textMuted,
            flexWrap: 'wrap',
            alignItems: 'center',
          }}
        >
          <span>
            <span
              style={{
                display: 'inline-block',
                width: 10,
                height: 10,
                borderRadius: 2,
                background: '#3b82f6',
                marginRight: 3,
                verticalAlign: 'middle',
              }}
            />{' '}
            Yours
          </span>
          <span>
            <span
              style={{
                display: 'inline-block',
                width: 10,
                height: 10,
                borderRadius: 2,
                background: '#22c55e',
                marginRight: 3,
                verticalAlign: 'middle',
              }}
            />{' '}
            Others
          </span>
          <span>
            <span
              style={{
                display: 'inline-block',
                width: 10,
                height: 10,
                borderRadius: 2,
                background: '#f97316',
                marginRight: 3,
                verticalAlign: 'middle',
              }}
            />{' '}
            Active
          </span>
          <span>
            <span
              style={{
                display: 'inline-block',
                width: 10,
                height: 10,
                borderRadius: 2,
                background: '#9ca3af',
                marginRight: 3,
                verticalAlign: 'middle',
                opacity: 0.5,
              }}
            />{' '}
            Pending
          </span>
        </div>
      </div>

      {/* Grid */}
      <div style={{ userSelect: 'none' }}>
        {/* Day headers — sticky */}
        <div
          style={{
            display: 'flex',
            borderBottom: `1px solid ${colors.cardBorder}`,
            position: 'sticky',
            top: 0,
            zIndex: 10,
            background: colors.cardBg,
          }}
        >
          <div style={{ width: TIME_COL_WIDTH, minWidth: TIME_COL_WIDTH, flexShrink: 0 }} />
          {days.map((day, i) => {
            const isToday = todayStr === day.toDateString();
            const dayKey = day.toISOString().slice(0, 10);
            return (
              <div
                key={dayKey}
                style={{
                  flex: 1,
                  textAlign: 'center',
                  padding: '0.4rem 0',
                  fontSize: '0.75rem',
                  fontWeight: isToday ? 700 : 500,
                  color: isToday ? colors.buttonPrimary : colors.textSecondary,
                  borderLeft: i > 0 ? `1px solid ${colors.cardBorder}` : undefined,
                }}
              >
                {formatDate(day)}
              </div>
            );
          })}
        </div>

        {/* Column-based body */}
        <div style={{ display: 'flex', position: 'relative' }}>
          {/* Time gutter */}
          <div style={{ width: TIME_COL_WIDTH, minWidth: TIME_COL_WIDTH, flexShrink: 0, position: 'relative', height: gridHeight }}>
            {hourLines.map((slot) => (
              <div
                key={slot}
                style={{
                  position: 'absolute',
                  top: (slot - visibleStart) * CELL_HEIGHT,
                  right: 4,
                  fontSize: '0.65rem',
                  color: colors.textMuted,
                  lineHeight: '1',
                  transform: 'translateY(-50%)',
                }}
              >
                {formatTime(slot)}
              </div>
            ))}
          </div>

          {/* Day columns */}
          {days.map((day, dayIdx) => {
            const isToday = todayStr === day.toDateString();
            const blocks = dayBlocks[dayIdx];
            const dayKey = day.toISOString().slice(0, 10);

            return (
              <div
                key={dayKey}
                style={{
                  flex: 1,
                  position: 'relative',
                  height: gridHeight,
                  borderLeft: `1px solid ${colors.cardBorder}`,
                  background: isToday ? `${colors.buttonPrimary}08` : undefined,
                }}
              >
                {/* Grid lines — every 2 hours */}
                {hourLines.map((slot) => (
                  <div
                    key={`line-${slot}`}
                    style={{
                      position: 'absolute',
                      top: (slot - visibleStart) * CELL_HEIGHT,
                      left: 0,
                      right: 0,
                      height: 0,
                      borderTop: `1px solid ${colors.cardBorder}`,
                      pointerEvents: 'none',
                    }}
                  />
                ))}

                {/* Reservation blocks (zIndex: 2) */}
                {blocks.map((block) => {
                  const top = (block.topSlot - visibleStart) * CELL_HEIGHT;
                  const height = (block.bottomSlot - block.topSlot + 1) * CELL_HEIGHT;
                  const bgColor = getReservationColor(block.reservation);
                  const isPending = block.reservation.status === 'pending';
                  const isShort = height < 56; // less than 2 slots = less than 1 hour

                  const label = getBlockLabel(block.reservation);
                  const timeLabel = `${formatTime(block.topSlot)}–${formatTime(block.bottomSlot + 1)}`;

                  return (
                    <div
                      key={block.reservation.id}
                      role={onReservationClick ? 'button' : undefined}
                      tabIndex={onReservationClick ? 0 : undefined}
                      onClick={() => onReservationClick?.(block.reservation)}
                      onKeyDown={onReservationClick ? (e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault();
                          onReservationClick(block.reservation);
                        }
                      } : undefined}
                      title={`${label} (${block.reservation.status})\n${timeLabel}${block.reservation.reason ? '\n' + block.reservation.reason : ''}`}
                      style={{
                        position: 'absolute',
                        top: top + 1,
                        left: 2,
                        right: 2,
                        height: height - 2,
                        background: bgColor,
                        opacity: isPending ? 0.5 : 0.85,
                        borderRadius: 4,
                        zIndex: 2,
                        overflow: 'hidden',
                        cursor: onReservationClick ? 'pointer' : 'default',
                        padding: '2px 4px',
                        boxSizing: 'border-box',
                        display: 'flex',
                        flexDirection: 'column',
                        justifyContent: 'center',
                      }}
                    >
                      {isShort ? (
                        <span
                          style={{
                            fontSize: '0.6rem',
                            fontWeight: 700,
                            color: '#fff',
                            textAlign: 'center',
                            lineHeight: '1',
                          }}
                        >
                          {getInitials(block.reservation)}
                        </span>
                      ) : (
                        <>
                          <span
                            style={{
                              fontSize: '0.65rem',
                              fontWeight: 600,
                              color: '#fff',
                              lineHeight: '1.2',
                              overflow: 'hidden',
                              textOverflow: 'ellipsis',
                              whiteSpace: 'nowrap',
                            }}
                          >
                            {label}
                          </span>
                          <span
                            style={{
                              fontSize: '0.55rem',
                              color: 'rgba(255,255,255,0.8)',
                              lineHeight: '1.2',
                            }}
                          >
                            {timeLabel}
                          </span>
                        </>
                      )}
                    </div>
                  );
                })}

                {/* Interaction cells (zIndex: 3, invisible, above reservation overlays for drag-to-select) */}
                {Array.from({ length: visibleSlots }, (_, i) => {
                  const slot = visibleStart + i;
                  const occupied = isSlotOccupied(dayIdx, slot);
                  const selected = isSelected(dayIdx, slot);
                  const hovered =
                    hoveredCell?.day === dayIdx && hoveredCell?.slot === slot && !occupied;

                  const cellBg = selected
                    ? colors.badgePurpleBg
                    : hovered
                      ? `${colors.buttonPrimary}10`
                      : 'transparent';

                  return (
                    <div
                      key={`cell-${slot}`}
                      onMouseDown={() => handleMouseDown(dayIdx, slot)}
                      onMouseMove={() => handleMouseMove(dayIdx, slot)}
                      onMouseEnter={() => setHoveredCell({ day: dayIdx, slot })}
                      onMouseLeave={() => setHoveredCell(null)}
                      style={{
                        position: 'absolute',
                        top: i * CELL_HEIGHT,
                        left: 0,
                        right: 0,
                        height: CELL_HEIGHT,
                        zIndex: 3,
                        cursor: onSlotSelect ? 'pointer' : 'default',
                        background: cellBg,
                        // Selection overlay has semi-transparent fill
                        opacity: selected ? 0.7 : 1,
                      }}
                    />
                  );
                })}

                {/* Now-line */}
                {nowSlotFraction && nowSlotFraction.dayIdx === dayIdx && (
                  <div
                    style={{
                      position: 'absolute',
                      top: nowSlotFraction.offset,
                      left: 0,
                      right: 0,
                      height: 2,
                      background: '#ef4444',
                      zIndex: 5,
                      pointerEvents: 'none',
                    }}
                  >
                    <div
                      style={{
                        position: 'absolute',
                        left: -3,
                        top: -3,
                        width: 8,
                        height: 8,
                        borderRadius: '50%',
                        background: '#ef4444',
                      }}
                    />
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
