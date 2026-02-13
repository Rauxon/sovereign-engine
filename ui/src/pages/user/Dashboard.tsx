import { useState, useEffect, useCallback } from 'react';
import { getUserUsage } from '../../api';
import type { UsageResponse } from '../../types';
import { useTheme } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';
import UsageTimelineChart from '../../components/charts/UsageTimelineChart';
import UsagePieChart from '../../components/charts/UsagePieChart';

type Period = 'hour' | 'day' | 'week' | 'month';

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toString();
}

/** Parse a backend timestamp (UTC, no Z suffix) as a proper UTC Date. */
function parseUTC(ts: string): Date {
  return new Date(ts.endsWith('Z') ? ts : ts + 'Z');
}

/** Format a Date as a display label appropriate for the selected period. */
function formatBucketLabel(d: Date, period: Period): string {
  switch (period) {
    case 'hour':
    case 'day':
      return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    case 'week':
    case 'month':
      return d.toLocaleDateString([], { month: 'short', day: 'numeric' });
  }
}

/**
 * Generate an ordered list of time-bucket labels covering the full period.
 * This ensures the chart X-axis is complete and evenly spaced even when
 * some buckets have no data.
 */
function generateBucketLabels(period: Period): string[] {
  const now = new Date();
  const labels: string[] = [];

  let startMs: number;
  let stepMs: number;
  let count: number;

  switch (period) {
    case 'hour':
      startMs = now.getTime() - 60 * 60 * 1000;
      stepMs = 60 * 1000;       // 1 minute
      count = 61;
      break;
    case 'day':
      startMs = now.getTime() - 24 * 60 * 60 * 1000;
      stepMs = 60 * 60 * 1000;  // 1 hour
      count = 25;
      break;
    case 'week':
      startMs = now.getTime() - 7 * 24 * 60 * 60 * 1000;
      stepMs = 24 * 60 * 60 * 1000; // 1 day
      count = 8;
      break;
    case 'month':
      startMs = now.getTime() - 30 * 24 * 60 * 60 * 1000;
      stepMs = 24 * 60 * 60 * 1000; // 1 day
      count = 31;
      break;
  }

  for (let i = 0; i < count; i++) {
    const t = new Date(startMs + i * stepMs);
    labels.push(formatBucketLabel(t, period));
  }

  return labels;
}

export default function Dashboard() {
  const { colors } = useTheme();
  const [usage, setUsage] = useState<UsageResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [period, setPeriod] = useState<Period>('day');

  const fetchUsage = useCallback(async (showSpinner = true) => {
    if (showSpinner) setLoading(true);
    setError(null);
    try {
      const data = await getUserUsage(period);
      setUsage(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load usage data');
    } finally {
      if (showSpinner) setLoading(false);
    }
  }, [period]);

  // Initial fetch + periodic refresh every 15s
  useEffect(() => {
    fetchUsage();
    const interval = setInterval(() => fetchUsage(false), 15_000);
    return () => clearInterval(interval);
  }, [fetchUsage]);

  if (loading) return <LoadingSpinner message="Loading usage data..." />;
  if (error) return <ErrorAlert message={error} onRetry={fetchUsage} />;
  if (!usage) return null;

  // Generate all expected time buckets for the period (zero-filled, evenly spaced)
  const bucketLabels = generateBucketLabels(period);

  // Pivot timeline data: one row per timestamp, one column per model
  const modelNames = [...new Set(usage.timeline.map((p) => p.model))];
  const makeModelRow = (label: string) => {
    const row: Record<string, string | number> = { label };
    for (const m of modelNames) row[m] = 0;
    return row;
  };
  const byTimestamp = new Map<string, Record<string, string | number>>(
    bucketLabels.map((label) => [label, makeModelRow(label)]),
  );
  for (const point of usage.timeline) {
    const label = formatBucketLabel(parseUTC(point.timestamp), period);
    if (!byTimestamp.has(label)) {
      byTimestamp.set(label, makeModelRow(label));
    }
    byTimestamp.get(label)![point.model] = point.requests;
  }
  const timelineData = [...byTimestamp.values()];

  const pieData = usage.by_model.map((m) => ({
    name: m.category_name || m.model_id,
    value: m.requests,
  }));

  // Pivot timeline data by API token: one row per timestamp, one column per token name
  const tokenNames = [...new Set(usage.timeline_by_token.map((p) => p.token_name))];
  const makeTokenRow = (label: string) => {
    const row: Record<string, string | number> = { label };
    for (const t of tokenNames) row[t] = 0;
    return row;
  };
  const byTokenTimestamp = new Map<string, Record<string, string | number>>(
    bucketLabels.map((label) => [label, makeTokenRow(label)]),
  );
  for (const point of usage.timeline_by_token) {
    const label = formatBucketLabel(parseUTC(point.timestamp), period);
    if (!byTokenTimestamp.has(label)) {
      byTokenTimestamp.set(label, makeTokenRow(label));
    }
    byTokenTimestamp.get(label)![point.token_name] = point.requests;
  }
  const byTokenTimelineData = [...byTokenTimestamp.values()];

  const byTokenPieData = usage.by_token.map((t) => ({
    name: t.token_name,
    value: t.requests,
  }));

  // Pivot timeline data for tokens: one row per timestamp, input_tokens + output_tokens columns
  const tokenTimestamp = new Map<string, Record<string, string | number>>(
    bucketLabels.map((label) => [label, { label, 'Input Tokens': 0, 'Output Tokens': 0 }]),
  );
  for (const point of usage.timeline) {
    const label = formatBucketLabel(parseUTC(point.timestamp), period);
    if (!tokenTimestamp.has(label)) {
      tokenTimestamp.set(label, { label, 'Input Tokens': 0, 'Output Tokens': 0 });
    }
    const row = tokenTimestamp.get(label)!;
    row['Input Tokens'] = (row['Input Tokens'] as number) + point.input_tokens;
    row['Output Tokens'] = (row['Output Tokens'] as number) + point.output_tokens;
  }
  const tokenTimelineData = [...tokenTimestamp.values()];

  const tokenPieData = [
    { name: 'Input Tokens', value: usage.summary.total_input_tokens },
    { name: 'Output Tokens', value: usage.summary.total_output_tokens },
  ].filter((d) => d.value > 0);

  const cardStyle: React.CSSProperties = {
    background: colors.cardBg,
    border: `1px solid ${colors.cardBorder}`,
    borderRadius: 8,
    padding: '1.25rem',
    textAlign: 'center',
    minWidth: 160,
  };

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1.5rem' }}>
        <h1 style={{ margin: 0 }}>Dashboard</h1>
        <div style={{ display: 'flex', gap: '0.5rem' }}>
          {(['hour', 'day', 'week', 'month'] as Period[]).map((p) => (
            <button
              key={p}
              onClick={() => setPeriod(p)}
              style={{
                padding: '0.4rem 0.8rem',
                border: `1px solid ${colors.inputBorder}`,
                borderRadius: 4,
                background: period === p ? colors.buttonPrimary : colors.cardBg,
                color: period === p ? '#fff' : colors.textSecondary,
                cursor: 'pointer',
                fontSize: '0.85rem',
                textTransform: 'capitalize',
              }}
            >
              {p}
            </button>
          ))}
        </div>
      </div>

      {/* Summary cards */}
      <div style={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit, minmax(180px, 1fr))', gap: '1rem', marginBottom: '2rem' }}>
        <div style={cardStyle}>
          <div style={{ fontSize: '0.85rem', color: colors.textMuted, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Total Requests</div>
          <div style={{ fontSize: '1.75rem', fontWeight: 700, color: colors.textPrimary, margin: '0.25rem 0' }}>{formatNumber(usage.summary.total_requests)}</div>
        </div>
        <div style={cardStyle}>
          <div style={{ fontSize: '0.85rem', color: colors.textMuted, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Input Tokens</div>
          <div style={{ fontSize: '1.75rem', fontWeight: 700, color: colors.textPrimary, margin: '0.25rem 0' }}>{formatNumber(usage.summary.total_input_tokens)}</div>
        </div>
        <div style={cardStyle}>
          <div style={{ fontSize: '0.85rem', color: colors.textMuted, textTransform: 'uppercase', letterSpacing: '0.05em' }}>Output Tokens</div>
          <div style={{ fontSize: '1.75rem', fontWeight: 700, color: colors.textPrimary, margin: '0.25rem 0' }}>{formatNumber(usage.summary.total_output_tokens)}</div>
        </div>
      </div>

      {/* Charts — Requests */}
      <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '2rem', marginBottom: '2rem' }}>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsageTimelineChart data={timelineData} models={modelNames} title="Model Usage" />
        </div>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsagePieChart data={pieData} title="Requests by Model" />
        </div>
      </div>

      {/* Charts — By Token */}
      <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '2rem', marginBottom: '2rem' }}>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsageTimelineChart data={byTokenTimelineData} models={tokenNames} title="Access Token Usage" />
        </div>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsagePieChart data={byTokenPieData} title="Requests by Token" />
        </div>
      </div>

      {/* Charts — Tokens */}
      <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '2rem' }}>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsageTimelineChart data={tokenTimelineData} models={['Input Tokens', 'Output Tokens']} title="Token Usage" />
        </div>
        <div style={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, borderRadius: 8, padding: '1rem' }}>
          <UsagePieChart data={tokenPieData} title="Token Split" />
        </div>
      </div>
    </div>
  );
}
