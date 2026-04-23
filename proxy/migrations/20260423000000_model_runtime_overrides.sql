-- Per-model llama-server CLI override JSON. See proxy/src/docker/runtime_overrides.rs.
-- Default '{}' means "use llama.cpp defaults".
ALTER TABLE models
ADD COLUMN runtime_overrides TEXT NOT NULL DEFAULT '{}'
CHECK (json_valid(runtime_overrides));
