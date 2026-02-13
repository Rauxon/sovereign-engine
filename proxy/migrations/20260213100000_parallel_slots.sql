-- Add parallel_slots to container_secrets so the proxy knows
-- how many concurrent inference slots each backend exposes.
ALTER TABLE container_secrets ADD COLUMN parallel_slots INTEGER NOT NULL DEFAULT 1;
