-- SWA-aware KV cache pre-aggregates for VRAM estimation.
--
-- For heterogeneous-attention models (e.g. Gemma 3/4 with alternating global
-- and sliding-window layers), the single `n_kv_heads` column is not enough to
-- estimate KV cache correctly: we need to know (a) which layers are sliding
-- vs full-context, and (b) their per-layer kv-head counts and key/value dims.
--
-- We pre-compute two per-token aggregates at ingestion time so the estimator
-- is a cheap multiply:
--   kv_bytes_per_token_global = Σ over full-context layers of
--                                 kv_heads_i × (key_len + val_len) × 2
--   kv_bytes_per_token_swa    = Σ over sliding-window layers of
--                                 kv_heads_i × (key_len_swa + val_len_swa) × 2
--
-- VRAM estimate becomes:
--   kv_bytes = (global_bpt × context + swa_bpt × min(context, sliding_window))
--              × parallel
--
-- Both columns are nullable: NULL means "no SWA-aware data available; fall
-- back to the legacy formula derived from n_layers × n_kv_heads × ...".
--
-- `sliding_window` was parsed from GGUF metadata since v1.5.0 but never
-- persisted — add it here so the estimator can read it.
ALTER TABLE models ADD COLUMN kv_bytes_per_token_global INTEGER;
ALTER TABLE models ADD COLUMN kv_bytes_per_token_swa INTEGER;
ALTER TABLE models ADD COLUMN sliding_window INTEGER;
