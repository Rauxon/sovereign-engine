-- Add context_length extracted from GGUF metadata during download
ALTER TABLE models ADD COLUMN context_length INTEGER;
