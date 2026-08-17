CREATE TABLE agent_installations (
  tenant_id TEXT NOT NULL,
  agent_id TEXT NOT NULL,
  installation_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  catalog_item_version TEXT NOT NULL,
  agent_version TEXT NOT NULL,
  protocol TEXT NOT NULL CHECK (protocol IN ('acp', 'native')),
  distribution_id TEXT NOT NULL,
  distribution_type TEXT NOT NULL CHECK (distribution_type IN ('system', 'binary', 'npx', 'uvx')),
  ownership TEXT NOT NULL CHECK (ownership IN ('system', 'managed')),
  install_dir TEXT,
  resolved_program TEXT NOT NULL,
  args_json TEXT NOT NULL,
  definition_json TEXT NOT NULL,
  integrity_json TEXT,
  source_registry TEXT NOT NULL,
  catalog_version TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
  installation_status TEXT NOT NULL CHECK (installation_status IN ('ready', 'incompatible', 'broken')),
  runtime_status TEXT NOT NULL CHECK (runtime_status IN ('unchecked', 'ready', 'runtime_missing', 'entry_missing', 'failed')),
  runtime_error_code TEXT,
  runtime_error_message TEXT,
  runtime_checked_at TEXT,
  protocol_status TEXT NOT NULL CHECK (protocol_status IN ('unchecked', 'ready', 'auth_required', 'failed', 'unsupported')),
  protocol_error_code TEXT,
  protocol_error_message TEXT,
  protocol_checked_at TEXT,
  model_status TEXT CHECK (model_status IS NULL OR model_status IN ('unchecked', 'ready', 'failed', 'unsupported')),
  model_error_code TEXT,
  model_checked_at TEXT,
  installed_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY (tenant_id, agent_id),
  FOREIGN KEY (tenant_id) REFERENCES tenants(id) ON DELETE CASCADE,
  CHECK ((ownership = 'system' AND install_dir IS NULL) OR (ownership = 'managed' AND install_dir IS NOT NULL))
);

CREATE INDEX idx_agent_installations_ready
  ON agent_installations (tenant_id, enabled, installation_status, protocol_status);

CREATE UNIQUE INDEX idx_agent_installations_identity
  ON agent_installations (tenant_id, installation_id);
