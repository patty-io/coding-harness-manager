import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { SyncToHarnessButton } from "../components/SyncToHarnessButton";
import { useAdoptCanonical, useImportSkills, useSkills } from "../hooks/useSkills";
import { detectSkills } from "../lib/api";

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

      <DetectedSkillsSection />
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

function DetectedSkillsSection() {
  const [open, setOpen] = useState(false);
  const { data: detected, isLoading } = useQuery({
    queryKey: ["detected-skills"],
    queryFn: detectSkills,
    enabled: open,
  });
  const notInLibrary = (detected ?? []).filter((d) => !d.inLibrary);

  return (
    <div className="mt-4 rounded border border-slate-700 bg-slate-800/60 p-4">
      <button
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between text-left"
      >
        <span className="font-medium text-slate-200">
          Detected in your harnesses
          {notInLibrary.length > 0 && (
            <span className="ml-2 rounded bg-blue-500/15 px-2 py-0.5 text-xs text-blue-300">
              {notInLibrary.length} not in library
            </span>
          )}
        </span>
        <span className="text-xs text-slate-400">{open ? "hide" : "show"}</span>
      </button>
      {open && (
        <>
          {isLoading && <p className="mt-3 text-sm text-slate-400">Reading harness configs…</p>}
          {detected && detected.length === 0 && (
            <p className="mt-3 text-sm text-slate-500">No skills found in any harness config.</p>
          )}
          <ul className="mt-3 space-y-1.5">
            {(detected ?? []).map((d) => (
              <li
                key={d.name}
                className="flex items-center gap-3 rounded border border-slate-700 bg-slate-900 px-3 py-2 text-sm"
              >
                <div className="min-w-0 flex-1">
                  <div className="text-slate-100">{d.name}</div>
                  <div className="text-xs text-slate-500">
                    found in {d.foundIn.join(", ")}
                  </div>
                </div>
                {d.inLibrary ? (
                  <span className="rounded bg-green-500/15 px-2 py-0.5 text-xs text-green-400">
                    in library
                  </span>
                ) : (
                  <span className="rounded bg-slate-700 px-2 py-0.5 text-xs text-slate-400">
                    not in library
                  </span>
                )}
              </li>
            ))}
          </ul>
          {notInLibrary.length > 0 && (
            <p className="mt-2 text-xs text-slate-500">
              Import a skill with "Scan ~/.agents/skills" above or a manual
              path to add it to the library.
            </p>
          )}
        </>
      )}
    </div>
  );
}
