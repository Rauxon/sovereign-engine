import { useState, useEffect, useRef, useMemo } from 'react';
import { updateModel } from '../../api';
import type { AdminModel, RuntimeOverrides } from '../../types';
import { useTheme } from '../../theme';
import ErrorAlert from '../common/ErrorAlert';
import {
  type DraftForm,
  overridesToDraft,
  parseDraft,
  buildCliPreview,
  payloadsEqual,
} from './runtimeOverrides';

type RuntimeOverridesEditorProps = Readonly<{
  model: AdminModel;
  onSaved: (overrides: RuntimeOverrides) => void;
  onCancel: () => void;
}>;

export default function RuntimeOverridesEditor({ model, onSaved, onCancel }: RuntimeOverridesEditorProps) {
  const { colors } = useTheme();
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    dialogRef.current?.showModal();
  }, []);

  const initialDraft = useMemo(() => overridesToDraft(model.runtime_overrides), [model.runtime_overrides]);
  const initialPayload = useMemo(() => parseDraft(initialDraft).payload, [initialDraft]);

  const [draft, setDraft] = useState<DraftForm>(initialDraft);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const { payload, errors } = useMemo(() => parseDraft(draft), [draft]);
  const cliPreview = useMemo(() => buildCliPreview(payload), [payload]);
  const hasErrors = Object.keys(errors).length > 0;
  const isDirty = !payloadsEqual(payload, initialPayload);
  const canSave = isDirty && !hasErrors && !saving;

  const setField = <K extends keyof DraftForm>(key: K, value: DraftForm[K]) => {
    setDraft((prev) => ({ ...prev, [key]: value }));
  };

  const handleSave = async () => {
    if (!canSave) return;
    setSaving(true);
    setSaveError(null);
    try {
      await updateModel(model.id, { runtime_overrides: payload });
      onSaved(payload);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : 'Failed to save runtime overrides');
      setSaving(false);
    }
  };

  // ---- Styles ----

  const inputStyle: React.CSSProperties = {
    width: '100%',
    padding: '0.5rem',
    border: `1px solid ${colors.inputBorder}`,
    borderRadius: 4,
    fontSize: '0.9rem',
    boxSizing: 'border-box',
    background: colors.inputBg,
    color: colors.textPrimary,
  };

  const labelStyle: React.CSSProperties = {
    display: 'block',
    marginBottom: '0.25rem',
    fontSize: '0.85rem',
    fontWeight: 600,
    color: colors.textSecondary,
  };

  const helpStyle: React.CSSProperties = {
    fontSize: '0.75rem',
    color: colors.textMuted,
    marginTop: '0.25rem',
    lineHeight: 1.4,
  };

  const fieldErrorStyle: React.CSSProperties = {
    fontSize: '0.75rem',
    color: colors.dangerText,
    marginTop: '0.2rem',
    fontWeight: 600,
  };

  const linkStyle: React.CSSProperties = {
    color: colors.link,
    textDecoration: 'underline',
  };

  const handleClose = () => {
    onCancel();
  };

  return (
    <>
      <style>{`.runtime-overrides-dialog::backdrop { background: ${colors.overlayBg}; }`}</style>
      <dialog
        ref={dialogRef}
        className="runtime-overrides-dialog"
        aria-label="Runtime overrides editor"
        style={{
          border: 'none',
          borderRadius: 8,
          padding: '1.5rem',
          maxWidth: 620,
          width: '92%',
          boxShadow: colors.dialogShadow,
          background: colors.dialogBg,
          color: 'inherit',
        }}
        onClose={handleClose}
        onClick={(e) => {
          if (e.target === e.currentTarget) onCancel();
        }}
      >
        <h3 style={{ margin: '0 0 0.25rem', color: colors.textPrimary }}>Runtime overrides</h3>
        <div style={{ fontSize: '0.85rem', color: colors.textMuted, marginBottom: '1rem' }}>
          {model.hf_repo}
          {model.filename && <span> / {model.filename}</span>}
        </div>

        {saveError && <ErrorAlert message={saveError} />}

        {/* cache_ram_mib */}
        <div style={{ marginBottom: '0.9rem' }}>
          <label htmlFor="ro-cache-ram" style={labelStyle}>Prompt cache size (MiB)</label>
          <input
            id="ro-cache-ram"
            type="number"
            inputMode="numeric"
            value={draft.cache_ram_mib}
            placeholder="default"
            onChange={(e) => setField('cache_ram_mib', e.target.value)}
            style={inputStyle}
            min={0}
            max={16384}
          />
          {errors.cache_ram_mib && <div style={fieldErrorStyle}>{errors.cache_ram_mib}</div>}
          <div style={helpStyle}>
            Server-level prompt LRU cache. <strong>Set to 0</strong> to disable &mdash; required workaround for the
            GGML_ASSERT crash on Gemma3 dense models. See{' '}
            <a
              href="https://github.com/ggml-org/llama.cpp/issues/21762"
              target="_blank"
              rel="noopener noreferrer"
              style={linkStyle}
            >
              llama.cpp #21762
            </a>
            .
          </div>
        </div>

        {/* swa_full */}
        <div style={{ marginBottom: '0.9rem' }}>
          <label htmlFor="ro-swa-full" style={labelStyle}>Full SWA cache</label>
          <select
            id="ro-swa-full"
            value={draft.swa_full}
            onChange={(e) => setField('swa_full', e.target.value as DraftForm['swa_full'])}
            style={inputStyle}
          >
            <option value="default">Default (unset)</option>
            <option value="true">Yes (--swa-full)</option>
            <option value="false">No</option>
          </select>
          <div style={helpStyle}>
            Allocate sliding-window attention cache at full size. Costs ~2&times; SWA KV memory.
          </div>
        </div>

        {/* ctx_checkpoints */}
        <div style={{ marginBottom: '0.9rem' }}>
          <label htmlFor="ro-ctx-checkpoints" style={labelStyle}>Context checkpoints</label>
          <input
            id="ro-ctx-checkpoints"
            type="number"
            inputMode="numeric"
            value={draft.ctx_checkpoints}
            placeholder="default (32)"
            onChange={(e) => setField('ctx_checkpoints', e.target.value)}
            style={inputStyle}
            min={0}
            max={128}
          />
          {errors.ctx_checkpoints && <div style={fieldErrorStyle}>{errors.ctx_checkpoints}</div>}
          <div style={helpStyle}>
            Per-slot KV snapshots for fast intra-conversation reuse. Default 32. Set 0 to disable.
          </div>
        </div>

        {/* cache_reuse */}
        <div style={{ marginBottom: '0.9rem' }}>
          <label htmlFor="ro-cache-reuse" style={labelStyle}>Cache reuse min chunk</label>
          <input
            id="ro-cache-reuse"
            type="number"
            inputMode="numeric"
            value={draft.cache_reuse}
            placeholder="default (0 = off)"
            onChange={(e) => setField('cache_reuse', e.target.value)}
            style={inputStyle}
            min={0}
            max={8192}
          />
          {errors.cache_reuse && <div style={fieldErrorStyle}>{errors.cache_reuse}</div>}
          <div style={helpStyle}>
            Min token chunk for KV-shifting cross-request reuse. Default 0 = off.
          </div>
        </div>

        {/* extra */}
        <div style={{ marginBottom: '0.9rem' }}>
          <label htmlFor="ro-extra" style={labelStyle}>Extra args (advanced)</label>
          <textarea
            id="ro-extra"
            value={draft.extra}
            placeholder={'--threads\n8'}
            onChange={(e) => setField('extra', e.target.value)}
            rows={4}
            style={{ ...inputStyle, fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace', resize: 'vertical' }}
          />
          <div style={helpStyle}>
            Raw additional <code>llama-server</code> flags. One argument per line. E.g. <code>--threads</code> then{' '}
            <code>8</code> on the next line.
          </div>
        </div>

        {/* CLI preview */}
        <div style={{ marginBottom: '1rem' }}>
          <span style={labelStyle}>CLI preview</span>
          <pre
            data-testid="cli-preview"
            style={{
              margin: 0,
              padding: '0.6rem 0.75rem',
              background: colors.inputBg,
              border: `1px solid ${colors.inputBorder}`,
              borderRadius: 4,
              fontSize: '0.8rem',
              fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
              color: colors.textPrimary,
              whiteSpace: 'pre-wrap',
              wordBreak: 'break-all',
              minHeight: '1.6rem',
            }}
          >
            {cliPreview || <span style={{ color: colors.textMuted }}>(no overrides — using llama.cpp defaults)</span>}
          </pre>
        </div>

        {/* Actions */}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.75rem', marginTop: '1rem' }}>
          <button
            type="button"
            onClick={onCancel}
            disabled={saving}
            style={{
              padding: '0.5rem 1rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: saving ? 'default' : 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            type="button"
            onClick={handleSave}
            disabled={!canSave}
            style={{
              padding: '0.5rem 1rem',
              background: canSave ? colors.buttonPrimary : colors.buttonPrimaryDisabled,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: canSave ? 'pointer' : 'default',
              fontWeight: 600,
            }}
          >
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </dialog>
    </>
  );
}
