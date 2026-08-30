import type { DetectedMcp } from "./api";

export interface DetectedMcpGroup {
  /** Stable logical identity used to render one row per MCP server name. */
  key: string;
  name: string;
  /** Distinct transport/target definitions reported by the harness adapters. */
  entries: DetectedMcp[];
  /** All harnesses where any definition of this server was found. */
  foundIn: string[];
  /** Library membership is name-based, so one library match covers the group. */
  inLibrary: boolean;
}

/**
 * Collapse detections for the same logical MCP server into one UI row.
 *
 * The detection command deliberately keeps different transport/target
 * definitions separate. We retain those entries here so the UI can show the
 * differences and let the user choose which configuration to add, while
 * avoiding one repeated top-level row per harness.
 */
export function groupDetectedMcps(detected: DetectedMcp[]): DetectedMcpGroup[] {
  const groups = new Map<string, DetectedMcpGroup>();

  for (const entry of detected) {
    const key = entry.name.trim().toLowerCase();
    const group = groups.get(key) ?? {
      key,
      name: entry.name,
      entries: [],
      foundIn: [],
      inLibrary: false,
    };

    group.entries.push(entry);
    group.inLibrary ||= entry.inLibrary;
    for (const harness of entry.foundIn) {
      if (!group.foundIn.includes(harness)) group.foundIn.push(harness);
    }
    groups.set(key, group);
  }

  return [...groups.values()].sort((a, b) => a.name.localeCompare(b.name));
}
