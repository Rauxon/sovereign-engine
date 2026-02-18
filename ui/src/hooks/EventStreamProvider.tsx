import { useState, useEffect, useRef, useMemo } from 'react';
import type { MetricsSnapshot } from '../types';
import { EventStreamContext, type ConnectionStatus } from './useEventStream';

const MIN_RECONNECT_MS = 3_000;
const MAX_RECONNECT_MS = 30_000;

/**
 * Provides a single EventSource to /api/user/events, merging metrics and
 * reservation-change signals into one SSE connection.
 *
 * Wrap around AuthenticatedApp so every component below can call useEventStream().
 */
export function EventStreamProvider({ children }: Readonly<{ children: React.ReactNode }>) {
  const [snapshot, setSnapshot] = useState<MetricsSnapshot | null>(null);
  const [reservationRevision, setReservationRevision] = useState(0);
  const [status, setStatus] = useState<ConnectionStatus>('connecting');
  const retriesRef = useRef(0);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const esRef = useRef<EventSource | null>(null);

  useEffect(() => {
    function connect() {
      esRef.current?.close();

      const es = new EventSource('/api/user/events');
      esRef.current = es;
      setStatus('connecting');

      es.addEventListener('metrics', (event: MessageEvent) => {
        try {
          const data: MetricsSnapshot = JSON.parse(event.data);
          setSnapshot(data);
          setStatus('connected');
          retriesRef.current = 0;
        } catch {
          // Ignore malformed events
        }
      });

      es.addEventListener('reservations_changed', () => {
        setReservationRevision((r) => r + 1);
        retriesRef.current = 0;
      });

      es.onerror = () => {
        es.close();
        esRef.current = null;
        setStatus('disconnected');

        const delay = Math.min(
          MIN_RECONNECT_MS * Math.pow(2, retriesRef.current),
          MAX_RECONNECT_MS,
        );
        retriesRef.current += 1;
        timerRef.current = setTimeout(connect, delay);
      };
    }

    // Delay SSE connection so it doesn't compete with critical page assets
    // during hard-refresh (browsers allow only 6 connections per domain on HTTP/1.1)
    timerRef.current = setTimeout(connect, 2_000);

    return () => {
      esRef.current?.close();
      esRef.current = null;
      if (timerRef.current) {
        clearTimeout(timerRef.current);
        timerRef.current = null;
      }
    };
  }, []);

  const value = useMemo(
    () => ({ snapshot, reservationRevision, status }),
    [snapshot, reservationRevision, status],
  );

  return (
    <EventStreamContext.Provider value={value}>
      {children}
    </EventStreamContext.Provider>
  );
}
