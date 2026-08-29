import { useMutation } from "@tanstack/react-query";
import { useState } from "react";
import { announceToast } from "../components/Toast";
import { exportDiagnostics, runDoctor, type DoctorCheck, type DoctorReport } from "../lib/api";

function CheckList({ checks }: { checks: DoctorCheck[] }) {
  return (
    <ul className="mt-2 space-y-1 text-xs">
      {checks.map((check) => (
        <li key={`${check.check}:${check.detail}`} className="flex gap-2">
          <span className={check.passed ? "text-green-400" : "text-red-400"}>
            {check.passed ? "✓" : "✗"}
          </span>
          <span className="text-slate-300">{check.check}</span>
          {check.detail && <span className="truncate text-slate-500">{check.detail}</span>}
        </li>
      ))}
    </ul>
  );
}

function ReportSection({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="rounded border border-slate-700 bg-slate-800 p-4">
      <h2 className="font-medium text-slate-100">{title}</h2>
      {children}
    </section>
  );
}

export default function DoctorScreen() {
  const [report, setReport] = useState<DoctorReport | null>(null);
  const [destination, setDestination] = useState("~/.coding-harness-manager");
  const doctor = useMutation({ mutationFn: runDoctor, onSuccess: setReport });
  const exportRun = useMutation({
    mutationFn: () => exportDiagnostics(destination),
    onSuccess: (path) => announceToast({ message: `Diagnostics exported to ${path}` }),
  });

  return (
    <div>
      <div className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h1 className="text-2xl font-bold">Doctor</h1>
          <p className="mt-1 text-sm text-slate-400">
            Read-only checks for harness files, provider connections, MCP servers and skills.
          </p>
        </div>
        <button
          type="button"
          onClick={() => doctor.mutate()}
          disabled={doctor.isPending}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white hover:bg-blue-500 disabled:opacity-50"
        >
          {doctor.isPending ? "Running checks…" : "Run checks"}
        </button>
      </div>

      <div className="mt-4 rounded border border-slate-700 bg-slate-800/60 p-4">
        <h2 className="font-medium text-slate-200">Export a redacted report</h2>
        <p className="mt-1 text-xs text-slate-500">
          Credentials are redacted. The folder is created if it does not exist.
        </p>
        <div className="mt-2 flex flex-wrap gap-2">
          <input
            value={destination}
            onChange={(e) => setDestination(e.target.value)}
            aria-label="Diagnostics destination folder"
            className="min-w-72 flex-1 rounded border border-slate-600 bg-slate-900 px-2 py-1 text-sm text-slate-200"
          />
          <button
            type="button"
            onClick={() => exportRun.mutate()}
            disabled={exportRun.isPending}
            className="rounded border border-slate-500 px-3 py-1 text-sm text-slate-200 hover:bg-slate-700 disabled:opacity-50"
          >
            {exportRun.isPending ? "Exporting…" : "Export diagnostics"}
          </button>
        </div>
        {exportRun.isError && <p className="mt-2 text-xs text-red-400">Export failed: {exportRun.error.message}</p>}
      </div>

      {doctor.isError && <p className="mt-4 text-sm text-red-400" role="alert">Doctor failed: {doctor.error.message}</p>}
      {report && (
        <div className="mt-4 space-y-3">
          <div className="rounded border border-slate-700 bg-slate-800/60 p-3 text-sm">
            <span className={report.summary.startsWith("issues") ? "text-amber-300" : "text-green-400"} role="status" aria-live="polite">{report.summary}</span>
            <span className="ml-3 text-xs text-slate-500">generated {new Date(report.generatedAt).toLocaleString()} · app {report.appVersion}</span>
          </div>
          {report.harnessChecks.map((group) => (
            <ReportSection key={`harness:${group.harnessType}`} title={`Harness · ${group.harnessType}${group.version ? ` ${group.version}` : ""}`}>
              <CheckList checks={group.checks} />
            </ReportSection>
          ))}
          {report.providerChecks.map((group) => (
            <ReportSection key={`provider:${group.providerName}:${group.endpointName}`} title={`Provider · ${group.providerName} / ${group.endpointName}`}>
              <CheckList checks={group.checks} />
            </ReportSection>
          ))}
          {report.mcpChecks.length > 0 && <ReportSection title="MCP servers"><CheckList checks={report.mcpChecks} /></ReportSection>}
          {report.skillChecks.length > 0 && <ReportSection title="Skills"><CheckList checks={report.skillChecks} /></ReportSection>}
        </div>
      )}
      {!report && !doctor.isPending && <p className="mt-5 text-sm text-slate-500">Run checks to inspect the current machine.</p>}
    </div>
  );
}
