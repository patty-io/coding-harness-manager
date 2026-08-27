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
