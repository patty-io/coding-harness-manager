import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex gap-4 py-2 text-sm">
      <span className="w-48 shrink-0 text-slate-500">{label}</span>
      <span className="min-w-0 break-all font-mono text-xs text-slate-200">
        {children}
      </span>
    </div>
  );
}

export default function SettingsScreen() {
  const [version, setVersion] = useState<string | null>(null);

  useEffect(() => {
    getVersion()
      .then(setVersion)
      .catch(() => setVersion("unknown"));
  }, []);

  const home = "~/.coding-harness-manager";

  return (
    <div>
      <h1 className="text-2xl font-bold">Settings</h1>

      <div className="mt-4 rounded border border-slate-700 bg-slate-800 p-4">
        <Row label="App version">{version ?? "…"}</Row>
        <Row label="Data directory">{home}</Row>
        <Row label="Database">{home}/chm.sqlite</Row>
        <Row label="Logs">{home}/logs/chm.log</Row>
        <Row label="Keychain service">coding-harness-manager</Row>
      </div>

      <p className="mt-3 text-xs text-slate-500">
        Version checks for newer releases will appear here.
      </p>

      <div className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4">
        <h2 className="font-medium text-slate-200">Navigation</h2>
        <p className="mt-1 text-sm text-slate-400">
          Mouse back/forward buttons and{" "}
          <kbd className="rounded bg-slate-700 px-1">⌘[</kbd> /{" "}
          <kbd className="rounded bg-slate-700 px-1">⌘]</kbd> (or ⌘← / ⌘→)
          move through page history.
        </p>
      </div>
    </div>
  );
}