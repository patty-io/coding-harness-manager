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
