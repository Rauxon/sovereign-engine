import { useState } from 'react';
import { useTheme } from '../../theme';

interface CopyButtonProps {
  text: string;
  label?: string;
}

export default function CopyButton({ text, label = 'Copy' }: Readonly<CopyButtonProps>) {
  const { colors } = useTheme();
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // Clipboard API unavailable (insecure context) — silent no-op.
      // Sovereign Engine runs over TLS so this path should not be reached.
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <button
      onClick={handleCopy}
      style={{
        padding: '0.4rem 0.8rem',
        background: copied ? colors.successText : colors.buttonPrimary,
        color: '#fff',
        border: 'none',
        borderRadius: 4,
        cursor: 'pointer',
        fontSize: '0.85rem',
        transition: 'background 0.2s',
      }}
    >
      {copied ? 'Copied!' : label}
    </button>
  );
}
