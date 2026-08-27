// Typed wrapper around Tauri invoke. Every backend command gets one function.

import { invoke } from "@tauri-apps/api/core";

export const TIER1_HARNESSES = [
  "claude-code",
  "codex",
  "opencode",
  "pi",
  "reasonix",
] as const;

export type HarnessType = (typeof TIER1_HARNESSES)[number] | (string & {});

export interface HarnessInstallation {
  id: string;
  harness_type: HarnessType;
  executable_path: string | null;
  version: string | null;
  config_path: string | null;
  detected_at: string;
  last_scanned_at: string | null;
  status: "detected" | "installed" | "config-missing" | "error";
}

export async function scanHarnesses(): Promise<HarnessInstallation[]> {
  return invoke<HarnessInstallation[]>("scan_harnesses");
}

export async function listInstallations(): Promise<HarnessInstallation[]> {
  return invoke<HarnessInstallation[]>("list_installations_cmd");
}
export interface DashboardStats {
  harnesses: number;
  providers: number;
  models: number;
  mcp: number;
  skills: number;
  drifted: number;
}

export async function dashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>("dashboard_stats");
}

// --- Import commands ---

export interface ParsedStateView {
  models: {
    native_id: string;
    remote_model_id: string;
    display_name: string;
    context_window: number | null;
  }[];
  mcp: { native_name: string; transport: string; command: string | null }[];
  skills: { name: string; symlinked: boolean }[];
  warnings: string[];
}

export interface ImportOptions {
  importModels: boolean;
  importMcp: boolean;
  importSkills: boolean;
}

export interface ImportReport {
  providersCreated: number;
  modelsImported: number;
  mcpImported: number;
  skillsImported: number;
  skillsSymlinked: number;
  duplicates: string[];
}

export async function readHarnessState(
  installationId: string,
): Promise<ParsedStateView> {
  return invoke<ParsedStateView>("read_harness_state", { installationId });
}

export async function importHarnessState(
  installationId: string,
  options: ImportOptions,
): Promise<ImportReport> {
  return invoke<ImportReport>("import_harness_state", {
    installationId,
    options,
  });
}

// --- Providers ---

export interface Provider {
  id: string;
  name: string;
  display_name: string;
  enabled: boolean;
  notes: string | null;
  created_at: string;
  updated_at: string;
}

export interface ProviderEndpoint {
  id: string;
  provider_id: string;
  name: string;
  base_url: string;
  protocol: string;
  discovery_path: string | null;
  auth_type: string;
  credential_ref: { id: string; kind: string; reference: string; created_at: string; updated_at: string } | null;
  headers: Record<string, unknown>;
  enabled: boolean;
  created_at: string;
  updated_at: string;
}

export interface ProviderCatalogModel {
  id: string;
  endpoint_id: string;
  remote_model_id: string;
  raw_metadata: unknown;
  canonical_model_id: string | null;
  match_confidence: number | null;
  first_seen_at: string;
  last_seen_at: string;
  missing_since: string | null;
  status: "available" | "new" | "missing" | "deprecated" | "unknown";
}

export interface ProviderSummary {
  endpoints: number;
  discoveredModels: number;
  myModels: number;
  health: string;
}

export interface DiscoverReport {
  total: number;
  added: number;
  updated: number;
}

export interface EndpointDiscoverOutcome {
  endpoint_id: string;
  endpoint_name: string;
  report: DiscoverReport | null;
  error: string | null;
}

export interface ProviderDiscoverReport {
  endpoints_attempted: number;
  endpoints_succeeded: number;
  total: number;
  added: number;
  updated: number;
  outcomes: EndpointDiscoverOutcome[];
}

export interface EndpointInput {
  providerId: string;
  name: string;
  baseUrl: string;
  protocol: string;
  discoveryPath: string | null;
  authType: string;
  credentialRefId: string | null;
  headers: Record<string, unknown>;
  enabled: boolean;
}

export async function createProvider(name: string, displayName: string): Promise<Provider> {
  return invoke<Provider>("create_provider_cmd", { name, displayName });
}
export async function listProviders(): Promise<Provider[]> {
  return invoke<Provider[]>("list_providers_cmd");
}
export async function updateProvider(
  id: string,
  displayName: string,
  enabled: boolean,
  notes: string | null,
): Promise<Provider> {
  return invoke<Provider>("update_provider_cmd", { id, displayName, enabled, notes });
}
export async function deleteProvider(id: string): Promise<void> {
  return invoke<void>("delete_provider_cmd", { id });
}
export async function listEndpoints(providerId: string): Promise<ProviderEndpoint[]> {
  return invoke<ProviderEndpoint[]>("list_endpoints_cmd", { providerId });
}
export async function createEndpoint(
  input: EndpointInput,
  envVarName?: string,
): Promise<ProviderEndpoint> {
  return invoke<ProviderEndpoint>("create_endpoint_cmd", {
    input,
    envVarName: envVarName ?? null,
  });
}
export async function saveApiKey(keyName: string, value: string): Promise<string> {
  return invoke<string>("save_api_key", { keyName, value });
}
export async function envVarSet(varName: string): Promise<boolean> {
  return invoke<boolean>("env_var_set", { varName });
}
export async function checkEndpointHealth(endpointId: string): Promise<string> {
  return invoke<string>("check_endpoint_health", { endpointId });
}
export async function discoverEndpointModels(endpointId: string): Promise<DiscoverReport> {
  return invoke<DiscoverReport>("discover_endpoint_models", { endpointId });
}
export async function discoverProviderModels(providerId: string): Promise<ProviderDiscoverReport> {
  return invoke<ProviderDiscoverReport>("discover_provider_models", { providerId });
}
export async function listCatalogModels(endpointId: string): Promise<ProviderCatalogModel[]> {
  return invoke<ProviderCatalogModel[]>("list_catalog_models_cmd", { endpointId });
}
export async function providerSummary(providerId: string): Promise<ProviderSummary> {
  return invoke<ProviderSummary>("provider_summary", { providerId });
}

// --- My Models ---

export interface ModelRouteView {
  id: string;
  endpoint_id: string;
  provider_name: string;
  remote_model_id: string;
  display_name: string;
  context_window: number | null;
  max_input: number | null;
  max_output: number | null;
  capabilities: unknown;
  overrides: unknown;
  enabled: boolean;
  identity_name: string | null;
  provenance: { source?: string } | null;
}

export interface CatalogView {
  id: string;
  endpoint_id: string;
  provider_name: string;
  endpoint_name: string;
  remote_model_id: string;
  status: string;
  matchConfidence: number | null;
  identity_name: string | null;
}

export interface RouteUpdateInput {
  displayName?: string;
  contextWindow?: number;
  maxInput?: number;
  maxOutput?: number;
  enabled?: boolean;
  capabilities?: unknown;
  overrides?: unknown;
}

export interface RouteCreateInput {
  endpointId: string;
  remoteModelId: string;
  displayName?: string;
  contextWindow?: number;
  maxInput?: number;
  maxOutput?: number;
  capabilities?: unknown;
}

export interface EnrichCandidate {
  modelsDevId: string;
  displayName: string;
  contextWindow: number | null;
  maxOutput: number | null;
  confidence: number;
}

export type EnrichOutcome =
  | { Matched: { confidence: number; identity_id: string; identity_name: string } }
  | { Ambiguous: { candidates: EnrichCandidate[]; current: unknown } }
  | "Unknown";

export async function listRoutes(): Promise<ModelRouteView[]> {
  return invoke<ModelRouteView[]>("list_routes_cmd");
}
export async function updateRouteCmd(id: string, input: RouteUpdateInput): Promise<void> {
  return invoke<void>("update_route_cmd", { id, input });
}
export async function deleteRouteCmd(id: string): Promise<void> {
  return invoke<void>("delete_route_cmd", { id });
}
export async function createRouteCmd(input: RouteCreateInput): Promise<string> {
  return invoke<string>("create_route_cmd", { input });
}
export async function listCatalogAll(): Promise<CatalogView[]> {
  return invoke<CatalogView[]>("list_catalog_all");
}
export async function addCatalogBatch(catalogIds: string[]): Promise<number> {
  return invoke<number>("add_catalog_batch", { catalogIds });
}
export async function enrichRoute(routeId: string): Promise<EnrichOutcome> {
  return invoke<EnrichOutcome>("enrich_route_cmd", { routeId });
}
export async function resolveEnrichment(routeId: string, identityId: string): Promise<void> {
  return invoke<void>("resolve_enrichment_cmd", { routeId, identityId });
}

// --- MCP ---

export interface McpServer {
  id: string;
  name: string;
  transport: string;
  command: string | null;
  args: string[];
  url: string | null;
  env: Record<string, unknown>;
  scope_type: string;
  scope_path: string | null;
  provenance: unknown;
  enabled: boolean;
}

export interface McpInput {
  name: string;
  transport: string;
  command: string | null;
  args: string[];
  url: string | null;
  env: Record<string, unknown>;
}

export interface CheckResult {
  check: string;
  passed: boolean;
  detail: string;
}

export async function createMcp(input: McpInput): Promise<McpServer> {
  return invoke<McpServer>("create_mcp_cmd", { input });
}
export async function listMcp(): Promise<McpServer[]> {
  return invoke<McpServer[]>("list_mcp_cmd");
}
export async function deleteMcp(id: string): Promise<void> {
  return invoke<void>("delete_mcp_cmd", { id });
}
export async function runMcpDiagnostics(mcpId: string): Promise<CheckResult[]> {
  return invoke<CheckResult[]>("run_mcp_diagnostics", { mcpId });
}

export async function bindMcp(installationId: string, mcpId: string): Promise<void> {
  return invoke<void>("bind_mcp_cmd", { installationId, mcpId });
}

// --- Skills ---

export interface ScannedSkillView {
  name: string;
  path: string;
  contentHash: string;
}

export interface ImportSkillReport {
  imported: number;
  duplicates: string[];
  conflicts: string[];
}

export interface BindOutcome {
  bindingType: string;
  targetPath: string;
}

export async function scanSkillsDir(dir: string): Promise<ScannedSkillView[]> {
  return invoke<ScannedSkillView[]>("scan_skills_dir_cmd", { dir });
}
export async function importSkills(paths: string[]): Promise<ImportSkillReport> {
  return invoke<ImportSkillReport>("import_skills_cmd", { paths });
}
export async function adoptCanonicalDir(): Promise<number> {
  return invoke<number>("adopt_canonical_dir");
}
export async function bindSkill(installationId: string, skillId: string): Promise<BindOutcome> {
  return invoke<BindOutcome>("bind_skill_cmd", { installationId, skillId });
}

export interface SkillView {
  id: string;
  name: string;
  canonicalPath: string;
  contentHash: string | null;
  sourceType: string;
  enabled: boolean;
}

export async function listSkills(): Promise<SkillView[]> {
  return invoke<SkillView[]>("list_skills_cmd");
}

// --- Profiles ---

export interface RoleMappingView {
  role: string;
  model: string;
}

export interface ProfileView {
  id: string;
  name: string;
  harnessType: string;
  modelRouteId: string | null;
  providerEndpointId: string | null;
  providerName: string | null;
  modelDisplay: string | null;
  env: Record<string, unknown>;
  roleMappings: RoleMappingView[];
}

export interface ProfileInput {
  name: string;
  harnessType: string;
  modelRouteId: string | null;
  providerEndpointId: string | null;
  env: Record<string, unknown>;
  roleMappings: { role: string; model: string }[];
}

export async function listProfiles(): Promise<ProfileView[]> {
  return invoke<ProfileView[]>("list_profiles_cmd");
}
export async function createProfile(input: ProfileInput): Promise<string> {
  return invoke<string>("create_profile_cmd", { input });
}
export async function deleteProfile(id: string): Promise<void> {
  return invoke<void>("delete_profile_cmd", { id });
}


// --- History / rollback ---

export interface SnapshotEntry {
  path: string;
  before: string | null;
  after: string | null;
}

export interface HistoryEntry {
  transactionId: string;
  transactionType: string;
  status: string;
  startedAt: string;
  summary: string | null;
  snapshots: SnapshotEntry[];
}

export interface RollbackReport {
  filesRestored: string[];
  newTransactionId: string;
}

export async function listHistory(limit?: number): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("list_history_cmd", { limit });
}
export async function rollbackTransaction(transactionId: string): Promise<RollbackReport> {
  return invoke<RollbackReport>("rollback_transaction_cmd", { transactionId });
}

export interface LaunchResult {
  pid: number | null;
  executable: string;
}

export async function launchProfile(profileId: string): Promise<LaunchResult> {
  return invoke<LaunchResult>("launch_profile_cmd", { profileId });
}
