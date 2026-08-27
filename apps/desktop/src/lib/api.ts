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
export async function createEndpoint(input: EndpointInput): Promise<ProviderEndpoint> {
  return invoke<ProviderEndpoint>("create_endpoint_cmd", { input });
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
