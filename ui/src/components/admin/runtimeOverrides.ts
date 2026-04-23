import type { RuntimeOverrides } from '../../types';

/** A draft form value where number fields are kept as strings to preserve "blank" vs "0". */
export interface DraftForm {
  cache_ram_mib: string;
  /** "default" = unset, "true" / "false" = explicit. */
  swa_full: 'default' | 'true' | 'false';
  ctx_checkpoints: string;
  cache_reuse: string;
  /** Each line is one CLI argument. */
  extra: string;
}

interface NumericField {
  key: 'cache_ram_mib' | 'ctx_checkpoints' | 'cache_reuse';
  min: number;
  max: number;
}

const NUMERIC_FIELDS: NumericField[] = [
  { key: 'cache_ram_mib', min: 0, max: 16384 },
  { key: 'ctx_checkpoints', min: 0, max: 128 },
  { key: 'cache_reuse', min: 0, max: 8192 },
];

/** Convert a stored RuntimeOverrides into the draft form. */
export function overridesToDraft(o: RuntimeOverrides | null | undefined): DraftForm {
  return {
    cache_ram_mib: o?.cache_ram_mib === undefined ? '' : String(o.cache_ram_mib),
    swa_full: o?.swa_full === undefined ? 'default' : o.swa_full ? 'true' : 'false',
    ctx_checkpoints: o?.ctx_checkpoints === undefined ? '' : String(o.ctx_checkpoints),
    cache_reuse: o?.cache_reuse === undefined ? '' : String(o.cache_reuse),
    extra: (o?.extra ?? []).join('\n'),
  };
}

export interface ParseResult {
  payload: RuntimeOverrides;
  errors: Partial<Record<keyof DraftForm, string>>;
}

/** Parse a draft form into a payload and a per-field validation map. */
export function parseDraft(draft: DraftForm): ParseResult {
  const errors: Partial<Record<keyof DraftForm, string>> = {};
  const payload: RuntimeOverrides = {};

  for (const { key, min, max } of NUMERIC_FIELDS) {
    const raw = draft[key].trim();
    if (raw === '') continue; // blank => omit field
    if (!/^-?\d+$/.test(raw)) {
      errors[key] = 'Must be an integer';
      continue;
    }
    const n = Number.parseInt(raw, 10);
    if (Number.isNaN(n)) {
      errors[key] = 'Must be an integer';
      continue;
    }
    if (n < min || n > max) {
      errors[key] = `Must be between ${min} and ${max}`;
      continue;
    }
    payload[key] = n;
  }

  if (draft.swa_full === 'true') payload.swa_full = true;
  else if (draft.swa_full === 'false') payload.swa_full = false;

  const extraLines = draft.extra
    .split('\n')
    .map((s) => s.trim())
    .filter((s) => s.length > 0);
  if (extraLines.length > 0) payload.extra = extraLines;

  return { payload, errors };
}

/** Render the CLI args that the payload would produce. */
export function buildCliPreview(payload: RuntimeOverrides): string {
  const parts: string[] = [];
  if (payload.cache_ram_mib !== undefined) parts.push('--cache-ram', String(payload.cache_ram_mib));
  if (payload.swa_full === true) parts.push('--swa-full');
  if (payload.swa_full === false) parts.push('--no-swa-full');
  if (payload.ctx_checkpoints !== undefined) parts.push('--ctx-checkpoints', String(payload.ctx_checkpoints));
  if (payload.cache_reuse !== undefined) parts.push('--cache-reuse', String(payload.cache_reuse));
  if (payload.extra && payload.extra.length > 0) parts.push(...payload.extra);
  return parts.join(' ');
}

/** Stable structural equality for two RuntimeOverrides payloads. */
export function payloadsEqual(a: RuntimeOverrides, b: RuntimeOverrides): boolean {
  return JSON.stringify(canonicalise(a)) === JSON.stringify(canonicalise(b));
}

function canonicalise(o: RuntimeOverrides): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  const keys = Object.keys(o).sort();
  for (const k of keys) {
    const v = (o as Record<string, unknown>)[k];
    if (v !== undefined) out[k] = v;
  }
  return out;
}
