import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid, Legend } from 'recharts';
import { useTheme } from '../../theme';

const COLORS = ['#2563eb', '#e11d48', '#16a34a', '#d97706', '#7c3aed', '#0891b2', '#be185d', '#65a30d'];

interface UsageTimelineChartProps {
  data: Record<string, number | string>[];
  models: string[];
  title?: string;
}

export default function UsageTimelineChart({ data, models, title }: Readonly<UsageTimelineChartProps>) {
  const { colors } = useTheme();

  if (data.length === 0) {
    return (
      <div style={{ textAlign: 'center', color: colors.textMuted, padding: '2rem' }}>
        {title && <h3 style={{ marginBottom: '0.5rem' }}>{title}</h3>}
        <p>No data available</p>
      </div>
    );
  }

  let xAxisInterval = 0;
  if (data.length > 30) xAxisInterval = Math.floor(data.length / 10);
  else if (data.length > 15) xAxisInterval = 2;

  return (
    <div>
      {title && <h3 style={{ marginBottom: '0.5rem' }}>{title}</h3>}
      <ResponsiveContainer width="100%" height={280}>
        <LineChart data={data}>
          <CartesianGrid strokeDasharray="3 3" stroke={colors.chartGrid} />
          <XAxis
            dataKey="label"
            tick={{ fontSize: 11, fill: colors.textMuted }}
            interval={xAxisInterval}
            angle={data.length > 15 ? -45 : 0}
            textAnchor={data.length > 15 ? 'end' : 'middle'}
            height={data.length > 15 ? 60 : 30}
          />
          <YAxis allowDecimals={false} tick={{ fontSize: 12, fill: colors.textMuted }} />
          <Tooltip contentStyle={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, color: colors.textPrimary }} />
          {models.length > 1 && <Legend />}
          {models.map((model, i) => (
            <Line
              key={model}
              type="monotone"
              dataKey={model}
              stroke={COLORS[i % COLORS.length]}
              strokeWidth={2}
              dot={data.length > 20 ? false : { r: 3 }}
              connectNulls
            />
          ))}
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
