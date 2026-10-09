ALTER TABLE assets ADD COLUMN detector_id TEXT NOT NULL DEFAULT 'legacy.classifier';
ALTER TABLE assets ADD COLUMN detector_version INTEGER NOT NULL DEFAULT 1;
