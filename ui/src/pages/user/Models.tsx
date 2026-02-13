import { useState, useEffect, useCallback, useRef } from 'react';
import {
  getUserModels,
  getDiskUsage,
  searchHfModels,
  getHfRepoFiles,
  startHfDownload,
  getHfDownloads,
  cancelHfDownload,
} from '../../api';
import type { AdminModel, DiskUsage, HfSearchResult, HfDownload, HfRepoFile } from '../../types';
import { useTheme } from '../../theme';
import type { ThemeColors } from '../../theme';
import LoadingSpinner from '../../components/common/LoadingSpinner';
import ErrorAlert from '../../components/common/ErrorAlert';

const POLL_INTERVAL_MS = 3000;

const TASK_OPTIONS = [
  'text-generation',
  'text2text-generation',
  'summarization',
  'translation',
  'question-answering',
  'fill-mask',
  'token-classification',
  'text-classification',
  'feature-extraction',
  'image-text-to-text',
];

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`;
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

// ---------------------------------------------------------------------------
// Disk Usage Bar
// ---------------------------------------------------------------------------

function DiskBar({ disk, colors }: { disk: DiskUsage; colors: ThemeColors }) {
  const pct = disk.total_bytes > 0
    ? (disk.used_bytes / disk.total_bytes) * 100
    : 0;
  const barColor = pct >= 95 ? '#ef4444' : pct >= 80 ? '#f59e0b' : '#22c55e';

  return (
    <div style={{ marginBottom: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '0.35rem', fontSize: '0.85rem' }}>
        <span>Disk: {pct.toFixed(1)}% used</span>
        <span>{formatBytes(disk.free_bytes)} free</span>
      </div>
      <div style={{ background: colors.progressBarBg, borderRadius: 4, height: 12, overflow: 'hidden' }}>
        <div style={{ width: `${Math.min(pct, 100)}%`, height: '100%', background: barColor, borderRadius: 4, transition: 'width 0.3s' }} />
      </div>
      {pct >= 95 && (
        <div style={{ color: colors.dangerText, fontSize: '0.8rem', marginTop: '0.25rem' }}>
          Downloads blocked — disk above 95%
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// HuggingFace Search Section
// ---------------------------------------------------------------------------

function HfSearch({
  onDownload,
  diskFull,
  activeRepos,
  colors,
}: {
  onDownload: (repo: string, files?: string[]) => void;
  diskFull: boolean;
  activeRepos: Set<string>;
  colors: ThemeColors;
}) {
  const [query, setQuery] = useState('');
  const [task, setTask] = useState('text-generation');
  const ggufOnly = true;
  const [results, setResults] = useState<HfSearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  // File picker state
  const [pickerRepo, setPickerRepo] = useState<string | null>(null);
  const [pickerFiles, setPickerFiles] = useState<HfRepoFile[]>([]);
  const [pickerLoading, setPickerLoading] = useState(false);

  const inputStyle: React.CSSProperties = {
    padding: '0.5rem 0.75rem',
    border: `1px solid ${colors.inputBorder}`,
    borderRadius: 6,
    fontSize: '0.9rem',
    background: colors.inputBg,
    color: colors.textPrimary,
  };

  const btnPrimary: React.CSSProperties = {
    padding: '0.5rem 1rem',
    background: colors.buttonPrimary,
    color: '#fff',
    border: 'none',
    borderRadius: 6,
    cursor: 'pointer',
    fontSize: '0.9rem',
    fontWeight: 500,
  };

  const PAGE_SIZE = 20;

  const handleSearch = async () => {
    if (!query.trim()) return;
    setSearching(true);
    setSearchError(null);
    setPickerRepo(null);
    setResults([]);
    setHasMore(false);
    try {
      const data = await searchHfModels(query.trim(), task, ggufOnly ? 'gguf' : undefined, 0, PAGE_SIZE);
      setResults(data.models);
      setHasMore(data.has_more);
    } catch (err) {
      setSearchError(err instanceof Error ? err.message : 'Search failed');
    } finally {
      setSearching(false);
    }
  };

  const handleLoadMore = async () => {
    setLoadingMore(true);
    try {
      const data = await searchHfModels(query.trim(), task, ggufOnly ? 'gguf' : undefined, results.length, PAGE_SIZE);
      setResults((prev) => [...prev, ...data.models]);
      setHasMore(data.has_more);
    } catch (err) {
      setSearchError(err instanceof Error ? err.message : 'Load more failed');
    } finally {
      setLoadingMore(false);
    }
  };

  const handleDownloadClick = async (repo: string) => {
    // For GGUF repos, show file picker; otherwise download all
    if (repo.toLowerCase().includes('gguf')) {
      setPickerRepo(repo);
      setPickerLoading(true);
      try {
        const files = await getHfRepoFiles(repo);
        // Show only .gguf files for GGUF repos
        setPickerFiles(files.filter((f) => f.path.endsWith('.gguf')));
      } catch (err) {
        setSearchError(err instanceof Error ? err.message : 'Failed to list files');
        setPickerRepo(null);
      } finally {
        setPickerLoading(false);
      }
    } else {
      onDownload(repo);
    }
  };

  return (
    <div style={{ marginBottom: '1.5rem' }}>
      <h2 style={{ fontSize: '1.1rem', margin: '0 0 0.75rem' }}>Search HuggingFace</h2>
      <div style={{ display: 'flex', gap: '0.5rem', flexWrap: 'wrap', marginBottom: '0.75rem' }}>
        <input
          style={{ ...inputStyle, flex: 1, minWidth: 200 }}
          placeholder="Search models..."
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
        />
        <select
          style={inputStyle}
          value={task}
          onChange={(e) => setTask(e.target.value)}
        >
          {TASK_OPTIONS.map((t) => (
            <option key={t} value={t}>{t}</option>
          ))}
        </select>
        <button style={btnPrimary} onClick={handleSearch} disabled={searching || !query.trim()}>
          {searching ? 'Searching...' : 'Search'}
        </button>
      </div>

      {searchError && <ErrorAlert message={searchError} />}

      {/* File picker for GGUF repos */}
      {pickerRepo && (
        <div style={{ marginBottom: '0.75rem', padding: '0.75rem', border: `1px solid ${colors.pickerBorder}`, borderRadius: 8, background: colors.pickerBg }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.5rem' }}>
            <span style={{ fontSize: '0.9rem', fontWeight: 500 }}>Select file from {pickerRepo}</span>
            <button
              style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: '1rem', color: colors.textMuted }}
              onClick={() => setPickerRepo(null)}
            >
              X
            </button>
          </div>
          {pickerLoading ? (
            <div style={{ fontSize: '0.85rem', color: colors.textMuted }}>Loading files...</div>
          ) : pickerFiles.length === 0 ? (
            <div style={{ fontSize: '0.85rem', color: colors.textMuted }}>No .gguf files found in this repo.</div>
          ) : (
            <div style={{ maxHeight: 200, overflowY: 'auto' }}>
              {pickerFiles.map((f) => (
                <div
                  key={f.path}
                  style={{
                    display: 'flex',
                    justifyContent: 'space-between',
                    alignItems: 'center',
                    padding: '0.35rem 0',
                    borderBottom: `1px solid ${colors.cardBorder}`,
                    fontSize: '0.85rem',
                  }}
                >
                  <span style={{ wordBreak: 'break-all' }}>{f.path}</span>
                  <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem', whiteSpace: 'nowrap', marginLeft: '0.5rem' }}>
                    <span style={{ color: colors.textMuted }}>{formatBytes(f.size)}</span>
                    <button
                      style={{ ...btnPrimary, padding: '0.25rem 0.5rem', fontSize: '0.8rem' }}
                      disabled={diskFull}
                      onClick={() => {
                        onDownload(pickerRepo!, [f.path]);
                        setPickerRepo(null);
                      }}
                    >
                      Download
                    </button>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {results.length > 0 && (
        <div style={{ overflowX: 'auto' }}>
          <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
            <thead>
              <tr style={{ borderBottom: `2px solid ${colors.cardBorder}`, textAlign: 'left' }}>
                <th style={{ padding: '0.5rem' }}>Model</th>
                <th style={{ padding: '0.5rem' }}>Task</th>
                <th style={{ padding: '0.5rem', textAlign: 'right' }}>Downloads</th>
                <th style={{ padding: '0.5rem', textAlign: 'right' }}>Likes</th>
                <th style={{ padding: '0.5rem', textAlign: 'right' }}></th>
              </tr>
            </thead>
            <tbody>
              {results.map((m) => {
                const downloading = activeRepos.has(m.id);
                return (
                  <tr key={m.id} style={{ borderBottom: `1px solid ${colors.tableRowBorder}` }}>
                    <td style={{ padding: '0.5rem', wordBreak: 'break-all' }}>{m.id}</td>
                    <td style={{ padding: '0.5rem', color: colors.textMuted }}>{m.pipeline_tag ?? '-'}</td>
                    <td style={{ padding: '0.5rem', textAlign: 'right' }}>{formatNumber(m.downloads)}</td>
                    <td style={{ padding: '0.5rem', textAlign: 'right' }}>{formatNumber(m.likes)}</td>
                    <td style={{ padding: '0.5rem', textAlign: 'right' }}>
                      <button
                        style={{
                          ...btnPrimary,
                          padding: '0.3rem 0.6rem',
                          fontSize: '0.8rem',
                          opacity: diskFull || downloading ? 0.5 : 1,
                        }}
                        disabled={diskFull || downloading}
                        onClick={() => handleDownloadClick(m.id)}
                        title={diskFull ? 'Disk full' : downloading ? 'Already downloading' : 'Download'}
                      >
                        {downloading ? 'Downloading...' : 'Download'}
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
          {hasMore && (
            <div style={{ textAlign: 'center', marginTop: '0.75rem' }}>
              <button
                style={{ ...btnPrimary, opacity: loadingMore ? 0.5 : 1 }}
                disabled={loadingMore}
                onClick={handleLoadMore}
              >
                {loadingMore ? 'Loading...' : 'Load More'}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Active Downloads Section
// ---------------------------------------------------------------------------

function ActiveDownloads({
  downloads,
  onCancel,
  colors,
}: {
  downloads: HfDownload[];
  onCancel: (id: string) => void;
  colors: ThemeColors;
}) {
  if (downloads.length === 0) return null;

  const btnDanger: React.CSSProperties = {
    padding: '0.35rem 0.75rem',
    background: colors.buttonDanger,
    color: '#fff',
    border: 'none',
    borderRadius: 6,
    cursor: 'pointer',
    fontSize: '0.8rem',
  };

  return (
    <div style={{ marginBottom: '1.5rem' }}>
      <h2 style={{ fontSize: '1.1rem', margin: '0 0 0.75rem' }}>Active Downloads</h2>
      {downloads.map((dl) => {
        const pct = dl.total_bytes > 0
          ? (dl.progress_bytes / dl.total_bytes) * 100
          : 0;
        const isActive = dl.status === 'downloading';
        const barColor =
          dl.status === 'failed' ? '#ef4444' :
          dl.status === 'cancelled' ? '#9ca3af' :
          dl.status === 'complete' ? '#22c55e' :
          '#2563eb';

        return (
          <div key={dl.id} style={{ marginBottom: '0.75rem', padding: '0.75rem', border: `1px solid ${colors.cardBorder}`, borderRadius: 8, background: colors.cardBg }}>
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '0.35rem' }}>
              <span style={{ fontSize: '0.9rem', fontWeight: 500, wordBreak: 'break-all' }}>{dl.hf_repo}</span>
              <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
                <span style={{ fontSize: '0.8rem', color: colors.textMuted }}>
                  {dl.status === 'complete' ? 'Complete' :
                   dl.status === 'failed' ? 'Failed' :
                   dl.status === 'cancelled' ? 'Cancelled' :
                   `${pct.toFixed(1)}%`}
                </span>
                {isActive && (
                  <button style={btnDanger} onClick={() => onCancel(dl.id)}>Cancel</button>
                )}
              </div>
            </div>
            <div style={{ background: colors.progressBarBg, borderRadius: 4, height: 8, overflow: 'hidden' }}>
              <div style={{ width: `${Math.min(pct, 100)}%`, height: '100%', background: barColor, borderRadius: 4, transition: 'width 0.3s' }} />
            </div>
            {dl.total_bytes > 0 && isActive && (
              <div style={{ fontSize: '0.75rem', color: colors.textMuted, marginTop: '0.2rem' }}>
                {formatBytes(dl.progress_bytes)} / {formatBytes(dl.total_bytes)}
              </div>
            )}
            {dl.error && (
              <div style={{ fontSize: '0.8rem', color: colors.dangerText, marginTop: '0.25rem' }}>{dl.error}</div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main Models Page
// ---------------------------------------------------------------------------

export default function Models() {
  const { colors } = useTheme();
  const [models, setModels] = useState<AdminModel[]>([]);
  const [disk, setDisk] = useState<DiskUsage | null>(null);
  const [downloads, setDownloads] = useState<HfDownload[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchModels = useCallback(async () => {
    try {
      const data = await getUserModels();
      setModels(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load models');
    }
  }, []);

  const fetchDisk = useCallback(async () => {
    try {
      const data = await getDiskUsage();
      setDisk(data);
    } catch {
      // Non-critical — disk bar just won't show
    }
  }, []);

  const fetchDownloads = useCallback(async () => {
    try {
      const data = await getHfDownloads();
      setDownloads(data);

      // If a download just completed, refresh models + disk
      const hasCompleted = data.some((d) => d.status === 'complete');
      if (hasCompleted) {
        fetchModels();
        fetchDisk();
      }
    } catch {
      // Non-critical
    }
  }, [fetchModels, fetchDisk]);

  // Initial load (loading state is initialised to true).
  // The setState calls within fetchModels/fetchDisk/fetchDownloads happen
  // asynchronously after awaiting the API response, not synchronously.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    Promise.all([fetchModels(), fetchDisk(), fetchDownloads()]).finally(() =>
      setLoading(false),
    );
  }, [fetchModels, fetchDisk, fetchDownloads]);

  // Poll downloads while any are active
  useEffect(() => {
    const hasActive = downloads.some((d) => d.status === 'downloading');

    if (hasActive && !pollRef.current) {
      pollRef.current = setInterval(fetchDownloads, POLL_INTERVAL_MS);
    } else if (!hasActive && pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }

    return () => {
      if (pollRef.current) {
        clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [downloads, fetchDownloads]);

  const handleDownload = async (hfRepo: string, files?: string[]) => {
    try {
      await startHfDownload({ hf_repo: hfRepo, files });
      fetchDownloads();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to start download');
    }
  };

  const handleCancel = async (id: string) => {
    try {
      await cancelHfDownload(id);
      fetchDownloads();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to cancel download');
    }
  };

  const diskFull = disk ? (disk.used_bytes / disk.total_bytes) * 100 >= 95 : false;
  const activeRepos = new Set(
    downloads.filter((d) => d.status === 'downloading').map((d) => d.hf_repo),
  );

  if (loading) return <LoadingSpinner message="Loading models..." />;

  return (
    <div>
      <h1>Models</h1>

      {error && <ErrorAlert message={error} onRetry={() => { setError(null); fetchModels(); }} />}

      {disk && <DiskBar disk={disk} colors={colors} />}

      <HfSearch onDownload={handleDownload} diskFull={diskFull} activeRepos={activeRepos} colors={colors} />

      <ActiveDownloads downloads={downloads.filter((d) => d.status === 'downloading')} onCancel={handleCancel} colors={colors} />

      <div style={{ marginBottom: '1.5rem' }}>
        <h2 style={{ fontSize: '1.1rem', margin: '0 0 0.75rem' }}>Registered Models</h2>
        {models.length === 0 ? (
          <p style={{ color: colors.textMuted }}>No models registered.</p>
        ) : (
          <div style={{ overflowX: 'auto' }}>
            <table style={{ width: '100%', borderCollapse: 'collapse', fontSize: '0.85rem' }}>
              <thead>
                <tr style={{ borderBottom: `2px solid ${colors.cardBorder}`, textAlign: 'left' }}>
                  <th style={{ padding: '0.5rem' }}>Repository</th>
                  <th style={{ padding: '0.5rem' }}>File</th>
                  <th style={{ padding: '0.5rem', textAlign: 'right' }}>Size</th>
                  <th style={{ padding: '0.5rem', textAlign: 'right' }}>Context</th>
                  <th style={{ padding: '0.5rem' }}>Backend</th>
                  <th style={{ padding: '0.5rem' }}>Status</th>
                </tr>
              </thead>
              <tbody>
                {models.map((model) => (
                  <tr key={model.id} style={{ borderBottom: `1px solid ${colors.tableRowBorder}` }}>
                    <td style={{ padding: '0.5rem', wordBreak: 'break-all' }}>{model.hf_repo}</td>
                    <td style={{ padding: '0.5rem', color: colors.textMuted, wordBreak: 'break-all', maxWidth: 200 }}>
                      {model.filename ?? '-'}
                    </td>
                    <td style={{ padding: '0.5rem', textAlign: 'right', whiteSpace: 'nowrap' }}>
                      {formatBytes(model.size_bytes)}
                    </td>
                    <td style={{ padding: '0.5rem', textAlign: 'right', whiteSpace: 'nowrap' }}>
                      {model.context_length ? formatNumber(model.context_length) : '-'}
                    </td>
                    <td style={{ padding: '0.5rem', whiteSpace: 'nowrap' }}>{model.backend_type}</td>
                    <td style={{ padding: '0.5rem' }}>
                      <span
                        style={{
                          display: 'inline-block',
                          padding: '0.15rem 0.5rem',
                          borderRadius: 12,
                          fontSize: '0.75rem',
                          fontWeight: 600,
                          background: model.loaded ? colors.badgeSuccessBg : colors.badgeNeutralBg,
                          color: model.loaded ? colors.badgeSuccessText : colors.badgeNeutralText,
                          whiteSpace: 'nowrap',
                        }}
                      >
                        {model.loaded ? 'Loaded' : 'Not Loaded'}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
