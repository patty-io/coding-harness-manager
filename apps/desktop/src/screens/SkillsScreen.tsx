import { useState } from "react";
import { SyncToHarnessButton } from "../components/SyncToHarnessButton";
import { useAdoptCanonical, useImportSkills, useSkills } from "../hooks/useSkills";

export default function SkillsScreen() {
  const { data: skills, isLoading } = useSkills();
  const importSkills = useImportSkills();
  const adopt = useAdoptCanonical();
  const [manualPath, setManualPath] = useState("");
  const [message, setMessage] = useState<string | null>(null);

  const importManual = () => {
    if (!manualPath.trim()) return;
    importSkills.mutate([manualPath.trim()], {
      onSuccess: (r) => {
        setMessage(
          `Imported ${r.imported}; duplicates: ${r.duplicates.length}; conflicts: ${r.conflicts.length}`,
        );
        setManualPath("");
      },
      onError: (e) => setMessage(e.message),
    });
  };

  return (
    <div>
      <h1 className="text-2xl font-bold">Skills</h1>

      <div className="mt-4 flex flex-wrap items-center gap-2 text-sm">
        <button
          onClick={() => adopt.mutate()}
          disabled={adopt.isPending}
          className="rounded bg-blue-600 px-3 py-1 text-white disabled:opacity-50"
        >
          {adopt.isPending ? "Scanning…" : "Scan ~/.agents/skills"}
        </button>
        <input
          value={manualPath}
          onChange={(e) => setManualPath(e.target.value)}
          placeholder="/path/to/skill-dir"
          className="rounded border border-slate-600 px-2 py-1 font-mono text-xs"
        />
        <button
          onClick={importManual}
          disabled={!manualPath.trim() || importSkills.isPending}
          className="rounded border border-slate-600 px-3 py-1 disabled:opacity-50"
        >
          Import path
        </button>
      </div>
      {adopt.data !== undefined && (
        <p className="mt-2 text-sm text-green-700">{adopt.data} skills adopted</p>
      )}
      {message && <p className="mt-2 text-sm text-slate-200">{message}</p>}

      {isLoading && <p className="mt-4">Loading…</p>}
      <table className="mt-4 w-full bg-slate-800 text-sm">
        <thead>
          <tr className="border-b text-left">
            <th className="p-2">Name</th>
            <th className="p-2">Canonical path</th>
            <th className="p-2">Hash</th>
            <th className="p-2">Source</th>
            <th className="p-2">Actions</th>
          </tr>
        </thead>
        <tbody>
          {(skills ?? []).map((s) => (
            <tr key={s.id} className="border-b">
              <td className="p-2 font-medium">{s.name}</td>
              <td className="p-2 font-mono text-xs text-slate-300">
                {s.canonicalPath}
              </td>
              <td className="p-2 font-mono text-xs">
                {s.contentHash?.slice(0, 8) ?? "—"}
              </td>
              <td className="p-2 text-xs">{s.sourceType}</td>
              <td className="p-2">
                <SyncToHarnessButton />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}