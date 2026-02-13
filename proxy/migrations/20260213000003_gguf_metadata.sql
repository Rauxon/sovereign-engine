-- Add GGUF architecture metadata for VRAM estimation
ALTER TABLE models ADD COLUMN n_layers INTEGER;
ALTER TABLE models ADD COLUMN n_heads INTEGER;
ALTER TABLE models ADD COLUMN n_kv_heads INTEGER;
ALTER TABLE models ADD COLUMN embedding_length INTEGER;
