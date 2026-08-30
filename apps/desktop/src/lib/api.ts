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
  harnessDetails: DashboardHarnessSummary[];
}

export interface DashboardHarnessSummary {
  installationId: string;
  models: number | null;
  mcp: number | null;
  skills: number | null;
  drifted: boolean;
  stateError: string | null;
}

export async function dashboardStats(): Promise<DashboardStats> {
  return invoke<DashboardStats>("dashboard_stats");
}

// --- Import commands ---

export interface ParsedStateView {
  providers: { native_provider_id?: string; base_url?: string | null; api?: string | null }[];
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

export async function readHarnessRawConfig(installationId: string): Promise<string> {
  return invoke<string>("read_harness_raw_config", { installationId });
}

export interface HarnessDrift {
  installationId: string;
  configPath: string | null;
  everSynced: boolean;
  drifted: boolean;
  currentContent: string | null;
  lastWrittenContent: string | null;
}

export async function harnessDrift(installationId: string): Promise<HarnessDrift> {
  return invoke<HarnessDrift>("harness_drift_cmd", { installationId });
}

export interface ActionView {
  kind: string;
  identity: string;
  action: string;
}

export interface FilePreview {
  path: string;
  before: string | null;
  after: string | null;
}

export interface SyncPreviewReport {
  summary: string;
  actions: ActionView[];
  files: FilePreview[];
  planHash: string;
  writableChanges: number;
  hasBlockers: boolean;
}

export interface SyncSelection {
  modelIds?: string[];
  mcpIds?: string[];
  skillIds?: string[];
}

export interface SyncApplyReport {
  summary: string;
  filesWritten: string[];
  linksCreated: string[];
  transactionId: string;
  validation: { ok: boolean; errors: string[] };
}

export async function syncPreview(
  installationId: string,
  mode: string,
  selection?: SyncSelection,
): Promise<SyncPreviewReport> {
  return invoke<SyncPreviewReport>("sync_preview", { installationId, mode, selection });
}

export async function syncApply(
  installationId: string,
  mode: string,
  force: boolean,
  planHash: string,
  selection?: SyncSelection,
): Promise<SyncApplyReport> {
  return invoke<SyncApplyReport>("sync_apply", {
    installationId,
    mode,
    force,
    planHash,
    selection,
  });
}

export async function recordManualSnapshot(installationId: string): Promise<void> {
  return invoke<void>("record_manual_snapshot_cmd", { installationId });
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
  endpointId: string;
  endpointName: string;
  report: DiscoverReport | null;
  error: string | null;
}

export interface DiscoveredModel {
  catalogId: string;
  endpointId: string;
  endpointName: string;
  providerName: string;
  remoteModelId: string;
  displayName: string | null;
  contextLength: number | null;
  status: string;
}

export interface SkippedEndpoint {
  endpointId: string;
  endpointName: string;
  reason: string;
}

export interface ProviderDiscoverReport {
  endpointsAttempted: number;
  endpointsSucceeded: number;
  endpointsSkipped: SkippedEndpoint[];
  total: number;
  added: number;
  updated: number;
  distinctModels: number;
  newModels: DiscoveredModel[];
  updatedModels: DiscoveredModel[];
  outcomes: EndpointDiscoverOutcome[];
}

export interface AddToMyModelsReport {
  requested: number;
  created: number;
  alreadyRouted: number;
  failures: string[];
}

export interface ProviderCatalogEntry {
  catalogId: string;
  endpointId: string;
  endpointName: string;
  remoteModelId: string;
  displayName: string | null;
  contextLength: number | null;
  status: string;
  lastSeenAt: string;
  inMyModels: boolean;
  routeId: string | null;
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
export async function addDiscoveredToMyModels(catalogIds: string[]): Promise<AddToMyModelsReport> {
  return invoke<AddToMyModelsReport>("add_discovered_to_my_models_cmd", { catalogIds });
}
export async function listProviderCatalog(providerId: string): Promise<ProviderCatalogEntry[]> {
  return invoke<ProviderCatalogEntry[]>("list_provider_catalog_cmd", { providerId });
}
export async function listCatalogModels(endpointId: string): Promise<ProviderCatalogModel[]> {
  return invoke<ProviderCatalogModel[]>("list_catalog_models_cmd", { endpointId });
}
export async function providerSummary(providerId: string): Promise<ProviderSummary> {
  return invoke<ProviderSummary>("provider_summary", { providerId });
}
export interface ProviderSummaryEntry {
  providerId: string;
  summary: ProviderSummary;
}
export async function providerSummaries(): Promise<ProviderSummaryEntry[]> {
  return invoke<ProviderSummaryEntry[]>("provider_summaries");
}

// --- My Models ---

export interface ModelRouteView {
  id: string;
  endpoint_id: string;
  provider_name: string;
  endpoint_name: string;
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
  match_confidence: number | null;
  identity_name: string | null;
  in_my_models: boolean;
}

export interface DetectedMcp {
  name: string;
  transport: string;
  command: string | null;
  args: string[];
  url: string | null;
  env: Record<string, unknown>;
  foundIn: string[];
  inLibrary: boolean;
}

export interface DetectedSkill {
  name: string;
  foundIn: string[];
  paths: string[];
  inLibrary: boolean;
}

export async function detectMcp(): Promise<DetectedMcp[]> {
  return invoke<DetectedMcp[]>("detect_mcp_cmd");
}
export async function detectSkills(): Promise<DetectedSkill[]> {
  return invoke<DetectedSkill[]>("detect_skills_cmd");
}

export interface HarnessModelRow {
  nativeId: string;
  nativeProviderId: string | null;
  remoteModelId: string;
  displayName: string;
  contextWindow: number | null;
  inLibrary: boolean;
  libraryRouteId: string | null;
  libraryDisplayName: string | null;
  providerName: string | null;
  providerMatch:
    | "harness"
    | "library"
    | "catalog"
    | "library-suffix"
    | "catalog-suffix"
    | null;
  providerBaseUrl: string | null;
  providerId: string | null;
}

export interface HarnessModelOp {
  op: "update" | "remove" | "duplicate";
  nativeId: string;
  nativeProviderId?: string;
  destinationProviderId?: string;
  displayName?: string;
  contextWindow?: number;
  remoteModelId?: string;
}

export interface HarnessEditReport {
  added: number;
  updated: number;
  removed: number;
  unchanged: number;
  filesWritten: string[];
}

export async function applyHarnessModelEdits(
  installationId: string,
  ops: HarnessModelOp[],
): Promise<HarnessEditReport> {
  return invoke<HarnessEditReport>("apply_harness_model_edits_cmd", {
    installationId,
    ops,
  });
}

export async function harnessModelsView(installationId: string): Promise<HarnessModelRow[]> {
  return invoke<HarnessModelRow[]>("harness_models_view_cmd", { installationId });
}

export interface AdoptOutcome {
  routeId: string;
  created: boolean;
}

export interface SmartAdoptOutcome {
  routeId: string;
  routeCreated: boolean;
  providerCreated: boolean;
  endpointCreated: boolean;
  providerName: string;
  endpointId: string;
}

export async function smartAdoptHarnessModel(
  installationId: string,
  nativeId: string,
  nativeProviderId?: string | null,
): Promise<SmartAdoptOutcome> {
  return invoke<SmartAdoptOutcome>("smart_adopt_harness_model_cmd", {
    installationId,
    nativeId,
    nativeProviderId,
  });
}
export interface EnsureProviderOutcome {
  providerId: string;
  providerCreated: boolean;
  endpointCreated: boolean;
}

export async function ensureProviderFromHarness(
  installationId: string,
  providerName: string,
): Promise<EnsureProviderOutcome> {
  return invoke<EnsureProviderOutcome>("ensure_provider_from_harness_cmd", {
    installationId,
    providerName,
  });
}

export interface HarnessProviderDetail {
  installationId: string;
  harnessType: string;
  providerName: string;
  baseUrl: string | null;
  models: string[];
  attributionConfidence: string;
}

export async function harnessProviderDetail(
  installationId: string,
  providerName: string,
): Promise<HarnessProviderDetail> {
  return invoke<HarnessProviderDetail>("harness_provider_detail_cmd", {
    installationId,
    providerName,
  });
}
export async function adoptHarnessModel(
  installationId: string,
  nativeId: string,
  endpointId: string,
  nativeProviderId?: string | null,
): Promise<AdoptOutcome> {
  return invoke<AdoptOutcome>("adopt_harness_model_cmd", {
    installationId,
    nativeId,
    endpointId,
    nativeProviderId,
  });
}

export interface EndpointOption {
  endpointId: string;
  providerName: string;
  endpointName: string;
  protocol: string;
}

export async function listEndpointOptions(): Promise<EndpointOption[]> {
  return invoke<EndpointOption[]>("list_endpoint_options_cmd");
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
export async function updateProfile(id: string, input: ProfileInput): Promise<void> {
  return invoke<void>("update_profile_cmd", { id, input });
}
export async function deleteProfile(id: string): Promise<void> {
  return invoke<void>("delete_profile_cmd", { id });
}

export interface SetItemView {
  itemType: string;
  itemId: string;
}

export interface SetView {
  id: string;
  name: string;
  description: string | null;
  items: SetItemView[];
}

export async function listSets(): Promise<SetView[]> {
  return invoke<SetView[]>("list_sets_cmd");
}
export async function createSet(name: string, description: string | null): Promise<string> {
  return invoke<string>("create_set_cmd", { name, description });
}
export async function deleteSet(setId: string): Promise<void> {
  return invoke<void>("delete_set_cmd", { setId });
}
export async function addSetItem(
  setId: string,
  itemType: string,
  itemId: string,
): Promise<void> {
  return invoke<void>("add_set_item_cmd", { setId, itemType, itemId });
}
export async function removeSetItem(
  setId: string,
  itemType: string,
  itemId: string,
): Promise<void> {
  return invoke<void>("remove_set_item_cmd", { setId, itemType, itemId });
}

// A set preview/apply uses the same wire contract as regular sync. Keep the
// names as aliases at this seam so callers can remain explicit about which
// workflow they are invoking without maintaining two subtly different
// representations of the same plan.
export type SetPreviewReport = SyncPreviewReport;
export type SetApplyReport = SyncApplyReport;

export async function applySetPreview(
  setId: string,
  installationId: string,
  mode: string,
): Promise<SetPreviewReport> {
  return invoke<SetPreviewReport>("apply_set_preview_cmd", { setId, installationId, mode });
}
export async function applySet(
  setId: string,
  installationId: string,
  mode: string,
  planHash: string,
): Promise<SetApplyReport> {
  return invoke<SetApplyReport>("apply_set_cmd", { setId, installationId, mode, planHash });
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
  canRollback: boolean;
  rollbackReason: string | null;
}

export interface RollbackReport {
  filesRestored: string[];
  newTransactionId: string;
}

export async function listHistory(limit?: number): Promise<HistoryEntry[]> {
  return invoke<HistoryEntry[]>("list_history_cmd", { limit });
}

export interface DoctorCheck {
  check: string;
  passed: boolean;
  detail: string;
}

export interface DoctorReport {
  generatedAt: string;
  appVersion: string;
  harnessChecks: { harnessType: string; version: string | null; checks: DoctorCheck[] }[];
  providerChecks: { providerName: string; endpointName: string; checks: DoctorCheck[] }[];
  mcpChecks: DoctorCheck[];
  skillChecks: DoctorCheck[];
  systemChecks: DoctorCheck[];
  summary: string;
}

export async function runDoctor(): Promise<DoctorReport> {
  return invoke<DoctorReport>("run_doctor_cmd");
}
export async function exportDiagnostics(destDir: string): Promise<string> {
  return invoke<string>("export_diagnostics_cmd", { destDir });
}

export async function backupNow(destDir: string): Promise<string> {
  return invoke<string>("backup_now_cmd", { destDir });
}
export async function listBackups(destDir: string): Promise<string[]> {
  return invoke<string[]>("list_backups_cmd", { destDir });
}

export async function restoreBackup(backupPath: string): Promise<string> {
  return invoke<string>("restore_backup_cmd", { backupPath });
}

export async function exportConfig(destDir: string, preferences?: Record<string, unknown>): Promise<string> {
  return invoke<string>("export_config_cmd", { destDir, preferences });
}

export interface ImportPreview {
  additions: { kind: string; identity: string; detail?: string }[];
  conflicts: { kind: string; identity: string; detail?: string }[];
  unchanged: { kind: string; identity: string; detail?: string }[];
}

export async function previewImport(filePath: string): Promise<ImportPreview> {
  return invoke<ImportPreview>("preview_import_cmd", { filePath });
}

export interface ImportResult {
  applied: number;
  skipped: string[];
  conflicts: string[];
  mode: string;
}

export async function importConfig(filePath: string, mode: "merge" | "replaceManaged"): Promise<ImportResult> {
  return invoke<ImportResult>("import_config_cmd", { filePath, mode });
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
