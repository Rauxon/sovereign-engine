import { PieChart, Pie, Cell, Tooltip, Legend, ResponsiveContainer } from 'recharts';
import { useTheme } from '../../theme';

const COLORS = ['#2563eb', '#7c3aed', '#db2777', '#ea580c', '#16a34a', '#0891b2', '#ca8a04', '#6366f1'];

interface PieDataItem {
  name: string;
  value: number;
}

interface UsagePieChartProps {
  data: PieDataItem[];
  title?: string;
}

export default function UsagePieChart({ data, title }: Readonly<UsagePieChartProps>) {
  const { colors } = useTheme();

  if (data.length === 0) {
    return (
      <div style={{ textAlign: 'center', color: colors.textMuted, padding: '2rem' }}>
        {title && <h3 style={{ marginBottom: '0.5rem' }}>{title}</h3>}
        <p>No data available</p>
      </div>
    );
  }

  return (
    <div>
      {title && <h3 style={{ marginBottom: '0.5rem', textAlign: 'center' }}>{title}</h3>}
      <ResponsiveContainer width="100%" height={280}>
        <PieChart>
          <Pie
            data={data}
            cx="50%"
            cy="50%"
            outerRadius={90}
            dataKey="value"
            nameKey="name"
            label={({ name, percent }: { name: string; percent: number }) =>
              `${name} (${(percent * 100).toFixed(0)}%)`
            }
            labelLine={false}
          >
            {data.map((_entry, index) => (
              <Cell key={`cell-${index}`} fill={COLORS[index % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip contentStyle={{ background: colors.cardBg, border: `1px solid ${colors.cardBorder}`, color: colors.textPrimary }} />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
