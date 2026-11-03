ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN adapter_id TEXT NOT NULL DEFAULT '';

ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN name TEXT NOT NULL DEFAULT '';

ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN publisher TEXT NOT NULL DEFAULT '';

ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN record_kind TEXT NOT NULL DEFAULT 'session';

ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN package_manifest_file TEXT NOT NULL DEFAULT 'conversation-adapter-package.json';

ALTER TABLE conversation_adapter_catalog_releases
ADD COLUMN adapter_manifest_file TEXT NOT NULL DEFAULT 'conversation-adapter.json';
