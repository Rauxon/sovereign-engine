import { useCallback, useMemo } from 'react';
import Markdown from 'react-markdown';
import remarkGfm from 'remark-gfm';
import { useTheme } from '../../theme';
import type { ThemeColors } from '../../theme';
import guideContent from '@docs/USER_GUIDE.md?raw';

/** Replace placeholder URLs with the actual origin the user is on. */
function resolveGuideContent(raw: string): string {
  const origin = window.location.origin;
  return raw.replace(/https:\/\/your-domain/g, origin);
}

function markdownStyles(colors: ThemeColors) {
  return {
    container: {
      fontFamily: 'system-ui, sans-serif',
      lineHeight: 1.7,
      color: colors.textPrimary,
      maxWidth: 800,
    } as const,
    h1: {
      fontSize: '2rem',
      fontWeight: 700,
      marginTop: '2rem',
      marginBottom: '1rem',
      paddingBottom: '0.5rem',
      borderBottom: `2px solid ${colors.cardBorder}`,
      color: colors.textPrimary,
    } as const,
    h2: {
      fontSize: '1.5rem',
      fontWeight: 600,
      marginTop: '2.5rem',
      marginBottom: '0.75rem',
      paddingBottom: '0.35rem',
      borderBottom: `1px solid ${colors.cardBorder}`,
      color: colors.textPrimary,
    } as const,
    h3: {
      fontSize: '1.2rem',
      fontWeight: 600,
      marginTop: '1.5rem',
      marginBottom: '0.5rem',
      color: colors.textPrimary,
    } as const,
    p: {
      marginTop: '0.5rem',
      marginBottom: '0.75rem',
      color: colors.textSecondary,
    } as const,
    a: {
      color: colors.link,
      textDecoration: 'none',
    } as const,
    blockquote: {
      margin: '1rem 0',
      padding: '0.75rem 1rem',
      borderLeft: `4px solid ${colors.buttonPrimary}`,
      background: colors.tableHeaderBg,
      borderRadius: '0 6px 6px 0',
      color: colors.textSecondary,
    } as const,
    code: {
      fontFamily: 'ui-monospace, SFMono-Regular, Menlo, Monaco, monospace',
      background: colors.tableHeaderBg,
      padding: '0.15rem 0.35rem',
      borderRadius: 4,
      fontSize: '0.85em',
      color: colors.textPrimary,
    } as const,
    pre: {
      background: colors.navBg,
      color: colors.navText,
      padding: '1rem',
      borderRadius: 8,
      overflow: 'auto',
      fontSize: '0.85rem',
      lineHeight: 1.5,
      margin: '1rem 0',
    } as const,
    table: {
      width: '100%',
      borderCollapse: 'collapse' as const,
      margin: '1rem 0',
      border: `1px solid ${colors.cardBorder}`,
      borderRadius: 8,
      overflow: 'hidden',
    } as const,
    th: {
      textAlign: 'left' as const,
      padding: '0.6rem 0.75rem',
      background: colors.tableHeaderBg,
      borderBottom: `1px solid ${colors.cardBorder}`,
      fontSize: '0.85rem',
      fontWeight: 600,
      color: colors.tableHeaderText,
    } as const,
    td: {
      padding: '0.6rem 0.75rem',
      borderBottom: `1px solid ${colors.tableRowBorder}`,
      fontSize: '0.9rem',
      color: colors.textSecondary,
    } as const,
    ul: {
      marginTop: '0.5rem',
      marginBottom: '0.75rem',
      paddingLeft: '1.5rem',
      color: colors.textSecondary,
    } as const,
    ol: {
      marginTop: '0.5rem',
      marginBottom: '0.75rem',
      paddingLeft: '1.5rem',
      color: colors.textSecondary,
    } as const,
    li: {
      marginBottom: '0.35rem',
    } as const,
    hr: {
      border: 'none',
      borderTop: `1px solid ${colors.cardBorder}`,
      margin: '2rem 0',
    } as const,
    strong: {
      color: colors.textPrimary,
      fontWeight: 600,
    } as const,
  };
}

export default function UserGuide() {
  const { colors } = useTheme();
  const styles = markdownStyles(colors);

  const resolvedContent = useMemo(() => resolveGuideContent(guideContent), []);

  const handleDownload = useCallback(() => {
    const blob = new Blob([resolvedContent], { type: 'text/markdown' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = 'SovereignEngine_UserGuide.md';
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  }, [resolvedContent]);

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0, color: colors.textPrimary }}>User Guide</h2>
        <button
          onClick={handleDownload}
          style={{
            padding: '0.5rem 1rem',
            background: colors.buttonPrimary,
            color: '#fff',
            border: 'none',
            borderRadius: 6,
            cursor: 'pointer',
            fontSize: '0.85rem',
            fontWeight: 600,
            display: 'flex',
            alignItems: 'center',
            gap: '0.4rem',
          }}
        >
          ↓ Download
        </button>
      </div>

      <div style={{
        background: colors.cardBg,
        border: `1px solid ${colors.cardBorder}`,
        borderRadius: 12,
        padding: '2rem',
      }}>
        <div style={styles.container}>
          <Markdown
            remarkPlugins={[remarkGfm]}
            components={{
              h1: ({ children }) => <h1 style={styles.h1}>{children}</h1>,
              h2: ({ children }) => <h2 style={styles.h2}>{children}</h2>,
              h3: ({ children }) => <h3 style={styles.h3}>{children}</h3>,
              p: ({ children }) => <p style={styles.p}>{children}</p>,
              a: ({ href, children }) => <a href={href} style={styles.a} target="_blank" rel="noopener noreferrer">{children}</a>,
              blockquote: ({ children }) => <blockquote style={styles.blockquote}>{children}</blockquote>,
              code: ({ className, children }) => {
                const isBlock = className?.startsWith('language-');
                if (isBlock) {
                  return <code style={{ fontFamily: styles.code.fontFamily, fontSize: styles.code.fontSize }}>{children}</code>;
                }
                return <code style={styles.code}>{children}</code>;
              },
              pre: ({ children }) => <pre style={styles.pre}>{children}</pre>,
              table: ({ children }) => <table style={styles.table}>{children}</table>,
              th: ({ children }) => <th style={styles.th}>{children}</th>,
              td: ({ children }) => <td style={styles.td}>{children}</td>,
              ul: ({ children }) => <ul style={styles.ul}>{children}</ul>,
              ol: ({ children }) => <ol style={styles.ol}>{children}</ol>,
              li: ({ children }) => <li style={styles.li}>{children}</li>,
              hr: () => <hr style={styles.hr} />,
              strong: ({ children }) => <strong style={styles.strong}>{children}</strong>,
            }}
          >
            {resolvedContent}
          </Markdown>
        </div>
      </div>
    </div>
  );
}
