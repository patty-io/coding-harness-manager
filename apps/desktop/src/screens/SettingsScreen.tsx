import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Link } from "react-router-dom";
import { useConfirm } from "../components/ConfirmDialog";
import { announceToast } from "../components/Toast";
import {
  backupNow,
  exportConfig,
  importConfig,
  listBackups,
  previewImport,
  restoreBackup,
  type ImportPreview,
} from "../lib/api";

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-center gap-4 py-2 text-sm">
      <span className="w-48 shrink-0 text-slate-500">{label}</span>
      <span className="min-w-0 break-all font-mono text-xs text-slate-200">{children}</span>
    </div>
  );
}

function CopyPath({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <button
      type="button"
      className="ml-2 rounded border border-slate-600 px-2 py-0.5 text-xs text-slate-300 hover:bg-slate-700"
      onClick={() => {
        if (!navigator.clipboard) return;
        void navigator.clipboard.writeText(value).then(() => {
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

export default function SettingsScreen() {
  const [version, setVersion] = useState<string | null>(null);
  const [backupDir, setBackupDir] = useState("~/.coding-harness-manager/backups");
  const [exportDir, setExportDir] = useState("~/.coding-harness-manager/exports");
  const [backupPath, setBackupPath] = useState("");
  const [backups, setBackups] = useState<string[]>([]);
  const [importPath, setImportPath] = useState("");
  const [importMode, setImportMode] = useState<"merge" | "replaceManaged">("merge");
  const [importPreview, setImportPreview] = useState<ImportPreview | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [retentionDays, setRetentionDays] = useState("90");
  const [confirmDestructive, setConfirmDestructive] = useState(true);
  const { confirm, confirmDialog } = useConfirm();

  useEffect(() => {
    getVersion().then(setVersion).catch(() => setVersion("unknown"));
    setRetentionDays(localStorage.getItem("chm.historyRetentionDays") ?? "90");
    setConfirmDestructive(localStorage.getItem("chm.confirmDestructive") !== "false");
    void listBackups(backupDir).then(setBackups).catch(() => setBackups([]));
  }, []);

  const savePreference = (key: string, value: string) => localStorage.setItem(key, value);
  const run = async (operation: () => Promise<string>, success: (value: string) => string) => {
    setStatus(null);
    try {
      const value = await operation();
      const message = success(value);
      setStatus(message);
      announceToast({ message, tone: "success" });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus(`Failed: ${message}`);
      announceToast({ message: `Settings action failed: ${message}`, tone: "error" });
    }
  };

  return (
    <div>
      <h1 className="text-2xl font-bold">Settings</h1>

      <section className="mt-4 rounded border border-slate-700 bg-slate-800 p-4" aria-labelledby="app-info-heading">
        <h2 id="app-info-heading" className="font-medium text-slate-200">About and paths</h2>
        <Row label="App version">{version ?? "…"}</Row>
        <Row label="Data directory">~/.coding-harness-manager <CopyPath value="~/.coding-harness-manager" /></Row>
        <Row label="Database">~/.coding-harness-manager/chm.sqlite <CopyPath value="~/.coding-harness-manager/chm.sqlite" /></Row>
        <Row label="Logs">~/.coding-harness-manager/logs/chm.log <CopyPath value="~/.coding-harness-manager/logs/chm.log" /></Row>
        <Row label="Keychain service">coding-harness-manager</Row>
      </section>

      <section className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4" aria-labelledby="preferences-heading">
        <h2 id="preferences-heading" className="font-medium text-slate-200">Safety preferences</h2>
        <label className="mt-3 flex items-center gap-2 text-sm text-slate-300">
          <input
            type="checkbox"
            checked={confirmDestructive}
            onChange={(event) => {
              setConfirmDestructive(event.target.checked);
              savePreference("chm.confirmDestructive", String(event.target.checked));
            }}
          />
          Confirm destructive actions
        </label>
        <label className="mt-3 flex items-center gap-2 text-sm text-slate-300">
          History retention (days)
          <input
            type="number"
            min="1"
            max="3650"
            value={retentionDays}
            onChange={(event) => {
              setRetentionDays(event.target.value);
              savePreference("chm.historyRetentionDays", event.target.value);
            }}
            className="w-24 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm"
          />
        </label>
        <p className="mt-2 text-xs text-slate-500">Preferences are stored locally and never include credential values in a portable export.</p>
      </section>

      <section className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4" aria-labelledby="backup-heading">
        <h2 id="backup-heading" className="font-medium text-slate-200">Database recovery</h2>
        <p className="mt-1 text-sm text-slate-400">Create a verified SQLite snapshot before migrations or recovery work.</p>
        <div className="mt-3 flex flex-wrap gap-2">
          <input aria-label="Backup directory" value={backupDir} onChange={(event) => setBackupDir(event.target.value)} className="min-w-72 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" />
          <button type="button" onClick={() => void run(async () => { const path = await backupNow(backupDir); setBackups(await listBackups(backupDir)); return path; }, (path) => `Backup written to ${path}`)} className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500">Backup now</button>
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <input aria-label="Backup file" value={backupPath} onChange={(event) => setBackupPath(event.target.value)} placeholder="path to .sqlite backup" className="min-w-72 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" />
          <button
            type="button"
            disabled={!backupPath.trim()}
            onClick={() => confirm("Restore database backup?", "The backup is integrity-checked, the current database is preserved, and the app must be restarted after restore.", () => run(() => restoreBackup(backupPath), (message) => `${message}. Restart the app to use the restored database.`), "Restore")}
            className="rounded border border-amber-500 px-3 py-1 text-sm text-amber-300 hover:bg-amber-500/10 disabled:opacity-50"
          >Restore backup…</button>
        </div>
        {backups.length > 0 && <details className="mt-3 text-xs text-slate-500"><summary className="cursor-pointer">Existing backups ({backups.length})</summary><ul className="mt-1 space-y-1">{backups.slice(0, 10).map((path) => <li key={path} className="break-all font-mono">{path}</li>)}</ul></details>}
      </section>

      <section className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4" aria-labelledby="portable-heading">
        <h2 id="portable-heading" className="font-medium text-slate-200">Portable configuration</h2>
        <p className="mt-1 text-sm text-slate-400">Export providers, endpoints, models, MCP, skills, profiles, and sets. Secret values are never read or exported.</p>
        <div className="mt-3 flex flex-wrap gap-2">
          <input aria-label="Export directory" value={exportDir} onChange={(event) => setExportDir(event.target.value)} className="min-w-72 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" />
          <button type="button" onClick={() => void run(() => exportConfig(exportDir, { historyRetentionDays: retentionDays, confirmDestructive }), (path) => `Configuration exported to ${path}`)} className="rounded bg-blue-600 px-3 py-1 text-sm text-white hover:bg-blue-500">Export configuration</button>
        </div>
        <div className="mt-3 flex flex-wrap gap-2">
          <input aria-label="Import file" value={importPath} onChange={(event) => { setImportPath(event.target.value); setImportPreview(null); }} placeholder="path to chm-export-*.json" className="min-w-72 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm" />
          <button type="button" disabled={!importPath.trim()} onClick={async () => { try { setImportPreview(await previewImport(importPath)); setStatus("Import preview ready. Nothing has been changed."); } catch (error) { setStatus(`Preview failed: ${error instanceof Error ? error.message : String(error)}`); } }} className="rounded border border-blue-500 px-3 py-1 text-sm text-blue-300 hover:bg-blue-500/10 disabled:opacity-50">Preview import</button>
        </div>
        {importPreview && (
          <div className="mt-3 rounded border border-slate-700 bg-slate-900/50 p-3 text-sm" aria-live="polite">
            <p>{importPreview.additions.length} additions · {importPreview.conflicts.length} conflicts · {importPreview.unchanged.length} unchanged</p>
            {importPreview.conflicts.length > 0 && (
              <ul className="mt-2 space-y-1 text-xs text-amber-300">
                {importPreview.conflicts.slice(0, 8).map((conflict) => (
                  <li key={conflict.kind + ":" + conflict.identity}>
                    {conflict.kind}: {conflict.identity}{conflict.detail ? " — " + conflict.detail : ""}
                  </li>
                ))}
                {importPreview.conflicts.length > 8 && <li>…and {importPreview.conflicts.length - 8} more</li>}
              </ul>
            )}
            <div className="mt-2 flex flex-wrap items-center gap-2">
              <select aria-label="Import mode" value={importMode} onChange={(event) => setImportMode(event.target.value as "merge" | "replaceManaged")} className="rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm">
                <option value="merge">Merge additions</option>
                <option value="replaceManaged">Replace managed</option>
              </select>
              <button
                type="button"
                disabled={importPreview.additions.length + importPreview.conflicts.length === 0}
                onClick={() =>
                  confirm(
                    "Apply configuration import?",
                    importMode === "replaceManaged"
                      ? "Additions will be imported and conflicting managed records will be updated to match the preview."
                      : "Only additions will be imported; conflicting records will remain unchanged.",
                    () =>
                      run(
                        () =>
                          importConfig(importPath, importMode).then(
                            (result) =>
                              result.applied +
                              " item(s) imported" +
                              (result.conflicts.length
                                ? "; " + result.conflicts.length + " conflict(s) left unchanged"
                                : ""),
                          ),
                        (message) => message,
                      ),
                    "Apply import",
                  )
                }
                className="rounded bg-blue-600 px-3 py-1 text-sm text-white disabled:opacity-50"
              >
                Apply {importMode === "replaceManaged" ? "replace" : "merge"}
              </button>
            </div>
          </div>
        )}
      </section>

      <section className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4">
        <h2 className="font-medium text-slate-200">Navigation</h2>
        <p className="mt-1 text-sm text-slate-400">Mouse back/forward buttons and <kbd className="rounded bg-slate-700 px-1">⌘[</kbd> / <kbd className="rounded bg-slate-700 px-1">⌘]</kbd> move through page history.</p>
      </section>

      <section className="mt-6 rounded border border-slate-700 bg-slate-800/60 p-4" aria-labelledby="diagnostics-heading">
        <h2 id="diagnostics-heading" className="font-medium text-slate-200">Diagnostics</h2>
        <p className="mt-1 text-sm text-slate-400">Doctor explains whether the app can safely read/write each harness and reach each provider.</p>
        <div className="mt-3 flex gap-3 text-sm"><Link to="/history" className="text-blue-400 hover:underline">Open History →</Link><Link to="/doctor" className="text-blue-400 hover:underline">Open Doctor →</Link></div>
      </section>
      {status && <p className="mt-3 text-sm text-slate-200" role="status">{status}</p>}
      {confirmDialog}
    </div>
  );
}
