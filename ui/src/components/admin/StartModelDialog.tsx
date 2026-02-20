import { useState, useEffect, useRef } from 'react';
import { estimateVram, startContainer } from '../../api';
import type { AdminModel, VramEstimate, ContainerStartRequest } from '../../types';
import { useTheme } from '../../theme';

type StartModelDialogProps = Readonly<{
  model: AdminModel;
  availableGpuTypes: string[];
  onStarted: () => void;
  onCancel: () => void;
}>

function formatMb(mb: number): string {
  if (mb >= 1024) return `${(mb / 1024).toFixed(1)} GB`;
  return `${mb} MB`;
}

function contextSizeOptions(maxContext: number): number[] {
  const opts: number[] = [];
  // Start from 4K, but if the model max is smaller, include smaller powers of 2
  const minStart = Math.min(4096, maxContext);
  let v = 128;
  while (v <= maxContext) {
    if (v >= minStart) opts.push(v);
    v *= 2;
  }
  return opts;
}

function formatContextSize(n: number): string {
  if (n >= 1024) return `${n / 1024}K`;
  return String(n);
}

const GPU_LABELS: Record<string, string> = {
  vulkan: 'Vulkan',
  none: 'CPU',
};

export default function StartModelDialog({ model, availableGpuTypes, onStarted, onCancel }: StartModelDialogProps) {
  const { colors } = useTheme();
  const dialogRef = useRef<HTMLDialogElement>(null);

  useEffect(() => {
    dialogRef.current?.showModal();
  }, []);

  const defaultContext = model.context_length
    ? Math.min(model.context_length, 4096)
    : 4096;

  const [selectedGpuType, setSelectedGpuType] = useState(availableGpuTypes[0] ?? 'none');
  const [contextSize, setContextSize] = useState(defaultContext);
  const [parallel, setParallel] = useState(1);
  const [gpuLayers, setGpuLayers] = useState(99);
  const [estimate, setEstimate] = useState<VramEstimate | null>(null);
  const [estimateError, setEstimateError] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);
  const [startError, setStartError] = useState<string | null>(null);

  // Debounce estimate calls
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    if (timerRef.current) clearTimeout(timerRef.current);
    timerRef.current = setTimeout(async () => {
      setEstimateError(null);
      try {
        const est = await estimateVram(model.id, contextSize, parallel);
        setEstimate(est);
      } catch (err) {
        setEstimateError(err instanceof Error ? err.message : 'Estimate failed');
      }
    }, 300);
    return () => { if (timerRef.current) clearTimeout(timerRef.current); };
  }, [model.id, contextSize, parallel]);

  const handleStart = async () => {
    setStarting(true);
    setStartError(null);
    try {
      const req: ContainerStartRequest = {
        model_id: model.id,
        backend_type: 'llamacpp',
        gpu_type: selectedGpuType,
        gpu_layers: gpuLayers,
        context_size: contextSize,
        parallel,
      };
      await startContainer(req);
      onStarted();
    } catch (err) {
      setStartError(err instanceof Error ? err.message : 'Failed to start');
      setStarting(false);
    }
  };

  const hasGpu = !!(estimate && estimate.gpu_total_mb > 0);
  const canFit = !estimate || estimate.fits;

  // VRAM bar segments
  const barSegments = estimate && hasGpu ? (() => {
    const total = estimate.gpu_total_mb;
    const otherUsed = estimate.gpu_used_mb;
    const thisModel = estimate.total_mb;
    const otherPct = (otherUsed / total) * 100;
    const modelPct = (thisModel / total) * 100;
    const overflows = otherUsed + thisModel > total;
    return { total, otherUsed, thisModel, otherPct, modelPct, overflows };
  })() : null;

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

  const handleClose = () => {
    onCancel();
  };

  return (
    <>
      <style>{`.start-model-dialog::backdrop { background: ${colors.overlayBg}; }`}</style>
      <dialog
        ref={dialogRef}
        className="start-model-dialog"
        style={{
          border: 'none',
          borderRadius: 8,
          padding: '1.5rem',
          maxWidth: 520,
          width: '90%',
          boxShadow: colors.dialogShadow,
          background: colors.dialogBg,
          color: 'inherit',
        }}
        onClose={handleClose}
        onClick={(e) => {
          if (e.target === e.currentTarget) onCancel();
        }}
      >
        {/* Header */}
        <h3 style={{ margin: '0 0 0.25rem', color: colors.textPrimary }}>Start Model</h3>
        <div style={{ fontSize: '0.85rem', color: colors.textMuted, marginBottom: '1rem' }}>
          {model.hf_repo}
          {model.filename && <span> / {model.filename}</span>}
          {model.size_bytes > 0 && <span> ({formatMb(Math.round(model.size_bytes / (1024 * 1024)))})</span>}
        </div>

        {/* Context size */}
        <div style={{ marginBottom: '0.75rem' }}>
          <label htmlFor="start-model-context-size" style={labelStyle}>Context Size</label>
          <select
            id="start-model-context-size"
            value={contextSize}
            onChange={(e) => setContextSize(Number.parseInt(e.target.value, 10))}
            style={inputStyle}
          >
            {contextSizeOptions(model.context_length || 4096).map((size) => (
              <option key={size} value={size}>
                {formatContextSize(size)} tokens{size === model.context_length ? ' (model max)' : ''}
              </option>
            ))}
          </select>
        </div>

        {/* Parallel sequences */}
        <div style={{ marginBottom: '0.75rem' }}>
          <label htmlFor="start-model-parallel" style={labelStyle}>Parallel Sequences</label>
          <select
            id="start-model-parallel"
            value={parallel}
            onChange={(e) => setParallel(Number.parseInt(e.target.value, 10))}
            style={inputStyle}
          >
            {[1, 2, 3, 4, 5, 6, 7, 8].map((n) => (
              <option key={n} value={n}>{n}</option>
            ))}
          </select>
        </div>

        {/* GPU info + layers */}
        <div style={{ display: 'flex', gap: '1rem', marginBottom: '0.75rem' }}>
          <div style={{ flex: 1 }}>
            <label htmlFor="start-model-gpu" style={labelStyle}>GPU</label>
            <select
              id="start-model-gpu"
              value={selectedGpuType}
              onChange={(e) => setSelectedGpuType(e.target.value)}
              style={inputStyle}
            >
              {availableGpuTypes.map((t) => (
                <option key={t} value={t}>{GPU_LABELS[t] ?? t}</option>
              ))}
            </select>
          </div>
          <div style={{ flex: 1 }}>
            <label htmlFor="start-model-gpu-layers" style={labelStyle}>GPU Layers</label>
            <input
              id="start-model-gpu-layers"
              type="number"
              value={gpuLayers}
              onChange={(e) => setGpuLayers(Math.max(0, Number.parseInt(e.target.value, 10) || 0))}
              min={0}
              style={inputStyle}
            />
          </div>
        </div>

        {/* VRAM estimation bar */}
        {estimate && hasGpu && barSegments && (
          <div style={{ marginBottom: '1rem' }}>
            <span style={labelStyle}>VRAM Estimate</span>
            <div style={{ background: colors.progressBarBg, borderRadius: 8, height: 28, overflow: 'hidden', position: 'relative', marginBottom: '0.35rem' }}>
              {/* Already used by other models */}
              <div
                style={{
                  position: 'absolute',
                  left: 0,
                  top: 0,
                  height: '100%',
                  width: `${Math.min(barSegments.otherPct, 100)}%`,
                  background: colors.successText,
                  transition: 'width 0.3s ease',
                }}
              />
              {/* This model's estimated usage */}
              <div
                style={{
                  position: 'absolute',
                  left: `${Math.min(barSegments.otherPct, 100)}%`,
                  top: 0,
                  height: '100%',
                  width: `${Math.min(barSegments.modelPct, 100 - Math.min(barSegments.otherPct, 100))}%`,
                  background: barSegments.overflows ? colors.dangerText : colors.buttonPrimary,
                  transition: 'width 0.3s ease',
                }}
              />
            </div>
            <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', color: colors.textMuted }}>
              <span style={{ color: colors.successText }}>Other: {formatMb(barSegments.otherUsed)}</span>
              <span style={{ color: barSegments.overflows ? colors.dangerText : colors.buttonPrimary }}>
                This model: {formatMb(barSegments.thisModel)}
              </span>
              <span>Total: {formatMb(barSegments.total)}</span>
            </div>
            {estimate.kv_cache_mb > 0 && (
              <div style={{ fontSize: '0.75rem', color: colors.textMuted, marginTop: '0.25rem' }}>
                Weights: {formatMb(estimate.model_weights_mb)} + KV cache: {formatMb(estimate.kv_cache_mb)} + overhead: {formatMb(estimate.overhead_mb)}
              </div>
            )}
            {barSegments.overflows && (
              <div style={{ fontSize: '0.8rem', color: colors.dangerText, fontWeight: 600, marginTop: '0.25rem' }}>
                Estimated VRAM exceeds available GPU memory
              </div>
            )}
          </div>
        )}

        {estimate && !hasGpu && (
          <div style={{ fontSize: '0.8rem', color: colors.textMuted, marginBottom: '1rem' }}>
            No GPU memory info available (nvidia-smi not found). Estimation unavailable.
          </div>
        )}

        {estimateError && (
          <div style={{ fontSize: '0.8rem', color: colors.dangerText, marginBottom: '0.75rem' }}>
            Estimate error: {estimateError}
          </div>
        )}

        {startError && (
          <div style={{ fontSize: '0.85rem', color: colors.dangerText, marginBottom: '0.75rem', fontWeight: 600 }}>
            {startError}
          </div>
        )}

        {/* Actions */}
        <div style={{ display: 'flex', justifyContent: 'flex-end', gap: '0.75rem', marginTop: '1rem' }}>
          <button
            onClick={onCancel}
            disabled={starting}
            style={{
              padding: '0.5rem 1rem',
              background: colors.buttonDisabled,
              color: colors.textSecondary,
              border: 'none',
              borderRadius: 4,
              cursor: starting ? 'default' : 'pointer',
            }}
          >
            Cancel
          </button>
          <button
            onClick={handleStart}
            disabled={starting || (hasGpu && !canFit)}
            style={{
              padding: '0.5rem 1rem',
              background: (starting || (hasGpu && !canFit)) ? colors.buttonPrimaryDisabled : colors.successText,
              color: '#fff',
              border: 'none',
              borderRadius: 4,
              cursor: (starting || (hasGpu && !canFit)) ? 'default' : 'pointer',
              fontWeight: 600,
            }}
          >
            {starting ? 'Starting...' : 'Start'}
          </button>
        </div>
      </dialog>
    </>
  );
}
