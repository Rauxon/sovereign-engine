-- Add explicit attention key/value dimensions for models with non-standard head_dim
-- (e.g. Gemma 4 where key_length != embedding_length / n_heads)
ALTER TABLE models ADD COLUMN key_length INTEGER;
ALTER TABLE models ADD COLUMN value_length INTEGER;
