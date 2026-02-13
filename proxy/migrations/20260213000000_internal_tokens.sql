-- Mark tokens as internal (e.g. auto-provisioned for Open WebUI)
ALTER TABLE tokens ADD COLUMN internal INTEGER NOT NULL DEFAULT 0;

-- Store tokenizer_config.json (or similar) captured during HF download
ALTER TABLE models ADD COLUMN model_metadata TEXT;
