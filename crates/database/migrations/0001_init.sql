CREATE TABLE providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  display_name TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1,
  notes TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE credential_refs (
  id TEXT PRIMARY KEY,
  type TEXT NOT NULL,
  reference TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE provider_endpoints (
  id TEXT PRIMARY KEY,
  provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  base_url TEXT NOT NULL,
  protocol TEXT NOT NULL,
  discovery_path TEXT,
  auth_type TEXT NOT NULL DEFAULT 'none',
  credential_ref_id TEXT REFERENCES credential_refs(id),
  headers_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE model_identities (
  id TEXT PRIMARY KEY,
  canonical_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  family TEXT,
  models_dev_id TEXT,
  metadata_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE provider_catalog_models (
  id TEXT PRIMARY KEY,
  endpoint_id TEXT NOT NULL REFERENCES provider_endpoints(id) ON DELETE CASCADE,
  remote_model_id TEXT NOT NULL,
  raw_metadata_json TEXT NOT NULL DEFAULT '{}',
  canonical_model_id TEXT REFERENCES model_identities(id),
  match_confidence INTEGER,
  first_seen_at TEXT NOT NULL,
  last_seen_at TEXT NOT NULL,
  missing_since TEXT,
  status TEXT NOT NULL DEFAULT 'available',
  UNIQUE (endpoint_id, remote_model_id)
);

CREATE TABLE model_routes (
  id TEXT PRIMARY KEY,
  endpoint_id TEXT NOT NULL REFERENCES provider_endpoints(id) ON DELETE CASCADE,
  model_identity_id TEXT REFERENCES model_identities(id),
  remote_model_id TEXT NOT NULL,
  display_name TEXT NOT NULL,
  context_window INTEGER,
  max_input INTEGER,
  max_output INTEGER,
  capabilities_json TEXT NOT NULL DEFAULT '{}',
  overrides_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  UNIQUE (endpoint_id, remote_model_id)
);

CREATE TABLE harness_installations (
  id TEXT PRIMARY KEY,
  harness_type TEXT NOT NULL,
  executable_path TEXT,
  version TEXT,
  config_path TEXT,
  detected_at TEXT NOT NULL,
  last_scanned_at TEXT,
  status TEXT NOT NULL DEFAULT 'detected',
  UNIQUE (harness_type)
);

CREATE TABLE harness_model_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  model_route_id TEXT NOT NULL REFERENCES model_routes(id) ON DELETE CASCADE,
  native_id TEXT NOT NULL,
  native_config_json TEXT NOT NULL DEFAULT '{}',
  managed INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE mcp_servers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  transport TEXT NOT NULL DEFAULT 'stdio',
  command TEXT,
  args_json TEXT NOT NULL DEFAULT '[]',
  url TEXT,
  env_json TEXT NOT NULL DEFAULT '{}',
  scope_type TEXT NOT NULL DEFAULT 'global',
  scope_path TEXT,
  provenance_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE harness_mcp_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  mcp_server_id TEXT NOT NULL REFERENCES mcp_servers(id) ON DELETE CASCADE,
  native_name TEXT NOT NULL,
  native_config_json TEXT NOT NULL DEFAULT '{}',
  managed INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE skills (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  canonical_path TEXT NOT NULL UNIQUE,
  source_type TEXT NOT NULL DEFAULT 'folder',
  source_url TEXT,
  content_hash TEXT,
  provenance_json TEXT NOT NULL DEFAULT '{}',
  enabled INTEGER NOT NULL DEFAULT 1,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE harness_skill_bindings (
  id TEXT PRIMARY KEY,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  skill_id TEXT NOT NULL REFERENCES skills(id) ON DELETE CASCADE,
  target_path TEXT NOT NULL,
  binding_type TEXT NOT NULL DEFAULT 'symlink',
  managed INTEGER NOT NULL DEFAULT 1,
  status TEXT NOT NULL DEFAULT 'active'
);

CREATE TABLE launch_profiles (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  harness_type TEXT NOT NULL,
  model_route_id TEXT REFERENCES model_routes(id),
  provider_endpoint_id TEXT REFERENCES provider_endpoints(id),
  env_json TEXT NOT NULL DEFAULT '{}',
  role_mappings_json TEXT NOT NULL DEFAULT '{}',
  native_overrides_json TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE configuration_sets (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  description TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE configuration_set_items (
  id TEXT PRIMARY KEY,
  configuration_set_id TEXT NOT NULL REFERENCES configuration_sets(id) ON DELETE CASCADE,
  item_type TEXT NOT NULL,
  item_id TEXT NOT NULL
);

CREATE TABLE sync_transactions (
  id TEXT PRIMARY KEY,
  transaction_type TEXT NOT NULL,
  started_at TEXT NOT NULL,
  completed_at TEXT,
  status TEXT NOT NULL DEFAULT 'running',
  summary TEXT,
  plan_json TEXT NOT NULL DEFAULT '{}',
  error_json TEXT
);

CREATE TABLE config_snapshots (
  id TEXT PRIMARY KEY,
  transaction_id TEXT NOT NULL REFERENCES sync_transactions(id) ON DELETE CASCADE,
  harness_installation_id TEXT NOT NULL REFERENCES harness_installations(id) ON DELETE CASCADE,
  path TEXT NOT NULL,
  before_content TEXT,
  after_content TEXT,
  before_hash TEXT,
  after_hash TEXT
);

CREATE INDEX idx_endpoints_provider ON provider_endpoints(provider_id);
CREATE INDEX idx_catalog_endpoint ON provider_catalog_models(endpoint_id);
CREATE INDEX idx_routes_endpoint ON model_routes(endpoint_id);
CREATE INDEX idx_mcp_binding_harness ON harness_mcp_bindings(harness_installation_id);
CREATE INDEX idx_skill_binding_harness ON harness_skill_bindings(harness_installation_id);
CREATE INDEX idx_binding_harness ON harness_model_bindings(harness_installation_id);
CREATE INDEX idx_snapshot_transaction ON config_snapshots(transaction_id);