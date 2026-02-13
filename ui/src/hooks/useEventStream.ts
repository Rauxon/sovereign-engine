import { createContext, useContext } from 'react';
import type { MetricsSnapshot } from '../types';

export type ConnectionStatus = 'connecting' | 'connected' | 'disconnected';

export interface EventStreamState {
  snapshot: MetricsSnapshot | null;
  reservationRevision: number;
  status: ConnectionStatus;
}

export const EventStreamContext = createContext<EventStreamState>({
  snapshot: null,
  reservationRevision: 0,
  status: 'connecting',
});

/**
 * Access the unified event stream state.
 * Must be used inside an EventStreamProvider.
 */
export function useEventStream(): EventStreamState {
  return useContext(EventStreamContext);
}
