import { useMemo, useState } from "react";
import { Link } from "react-router-dom";
import { useQueries } from "@tanstack/react-query";
import { readHarnessState, TIER1_HARNESSES } from "../lib/api";
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";
import { useImportHarnessState } from "../hooks/useImport";
import type { ImportOptions, ImportReport } from "../lib/api";

type Step = "welcome" | "scan" | "select" | "review" | "done";

const STEPS: Step[] = ["welcome", "scan", "select", "review", "done"];
const STEP_LABELS: Record<Step, string> = {
  welcome: "Welcome",
  scan: "Scan",
  select: "Select",
  review: "Review",
  done: "Done",
};

export default function ImportWizard() {
  const [step, setStep] = useState<Step>("welcome");
  const [selected, setSelected] = useState<string[]>([]);
  const [reports, setReports] = useState<ImportReport[]>([]);
  const [importError, setImportError] = useState<string | null>(null);
  const [importing, setImporting] = useState(false);
  const [includeModels, setIncludeModels] = useState(true);
  const [includeMcp, setIncludeMcp] = useState(true);
  const [includeSkills, setIncludeSkills] = useState(true);
  const [independentCommitsAcknowledged, setIndependentCommitsAcknowledged] = useState(false);
  const [runStatus, setRunStatus] = useState<Record<string, "pending" | "succeeded" | "failed">>({});
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();
  const reviews = useQueries({
    queries: selected.map((installationId) => ({
      queryKey: ["harness-state", installationId],
      queryFn: () => readHarnessState(installationId),
      enabled: step === "review" || step === "done",
    })),
  });
  const importMutation = useImportHarnessState();

  const tier1 = (installations ?? []).filter((i) =>
    TIER1_HARNESSES.includes(i.harness_type as (typeof TIER1_HARNESSES)[number]),
  );

  const stepIndex = STEPS.indexOf(step);

  const toggle = (id: string) =>
    setSelected((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );

  const reviewComplete =
    selected.length > 0 &&
    reviews.length === selected.length &&
    reviews.every((review) => review.isSuccess);

  const doImport = async () => {
    if (!reviewComplete) return;
    setImporting(true);
    setImportError(null);
    const options: ImportOptions = {
      importModels: includeModels,
      importMcp: includeMcp,
      importSkills: includeSkills,
    };
    const results = [];
    const errors: string[] = [];
    for (const id of selected) {
      setRunStatus((previous) => ({ ...previous, [id]: "pending" }));
      try {
        results.push(await importMutation.mutateAsync({ installationId: id, options }));
        setRunStatus((previous) => ({ ...previous, [id]: "succeeded" }));
      } catch (e) {
        setRunStatus((previous) => ({ ...previous, [id]: "failed" }));
        const harness = tier1.find((installation) => installation.id === id);
        errors.push(
          (harness?.harness_type ?? id) +
            ": " +
            (e instanceof Error ? e.message : String(e)),
        );
      }
    }
    setReports(results);
    setImporting(false);
    setImportError(errors.length > 0 ? errors.join(" · ") : null);
    if (results.length > 0 || errors.length > 0) setStep("done");
  };

  const reviewed = useMemo(
    () => reviews.flatMap((r, index) => (r.data ? [{ installationId: selected[index], state: r.data }] : [])),
    [reviews, selected],
  );
  const reviewLoading = reviews.some((r) => r.isLoading);
  const reviewError = reviews.find((r) => r.isError)?.error;
  const totalReview = reviewed.reduce(
    (acc, { state }) => ({
      providers: acc.providers + state.providers.length,
      models: acc.models + state.models.length,
      mcp: acc.mcp + state.mcp.length,
      skills: acc.skills + state.skills.length,
      warnings: acc.warnings + state.warnings.length,
    }),
    { providers: 0, models: 0, mcp: 0, skills: 0, warnings: 0 },
  );
  const total = reports.reduce(
    (acc, r) => ({
      providers: acc.providers + r.providersCreated,
      models: acc.models + r.modelsImported,
      mcp: acc.mcp + r.mcpImported,
      skills: acc.skills + r.skillsImported,
      symlinked: acc.symlinked + r.skillsSymlinked,
      duplicates: acc.duplicates.concat(r.duplicates),
    }),
    { providers: 0, models: 0, mcp: 0, skills: 0, symlinked: 0, duplicates: [] as string[] },
  );

  return (
    <div className="mx-auto max-w-2xl">
      <ol className="flex gap-2 text-xs text-slate-400">
        {STEPS.map((s, idx) => (
          <li
            key={s}
            className={idx <= stepIndex ? "font-medium text-blue-600" : undefined}
          >
            {idx + 1}. {STEP_LABELS[s]}
          </li>
        ))}
      </ol>

      {step === "welcome" && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">Welcome</h1>
          <p className="mt-2 text-slate-200">
            Coding Harness Manager scans your machine and imports your existing
            harness configuration into a central registry.{" "}
            <strong>Nothing on disk is modified during import</strong> — you
            review everything before any sync ever happens.
          </p>
          <button
            onClick={() => setStep("scan")}
            className="mt-4 rounded bg-blue-600 px-4 py-2 text-white"
          >
            Get started
          </button>
        </section>
      )}

      {step === "scan" && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">Scan your computer</h1>
          <p className="mt-2 text-slate-200">
            Detects installed coding harnesses and their configuration.
          </p>
          <button
            onClick={() => {
              scan.mutate(undefined, { onSuccess: () => setStep("select") });
            }}
            disabled={scan.isPending}
            className="mt-4 rounded bg-blue-600 px-4 py-2 text-white disabled:opacity-50"
          >
            {scan.isPending ? "Scanning…" : "Scan Computer"}
          </button>
          {scan.isError && (
            <p className="mt-2 text-red-600">
              Scan failed: {scan.error.message}
            </p>
          )}
          {isLoading && <p className="mt-2 text-slate-400">Loading…</p>}
        </section>
      )}

      {step === "select" && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">
            Found {tier1.length} supported harnesses
          </h1>
          <p className="mt-2 text-slate-200">
            Select which harnesses to import from (read-only).
          </p>
          <ul className="mt-4 space-y-2">
            {tier1.map((i) => (
              <li key={i.id}>
                <label className="flex items-center gap-2 rounded border border-slate-700 bg-slate-800 p-3">
                  <input
                    type="checkbox"
                    checked={selected.includes(i.id)}
                    onChange={() => toggle(i.id)}
                  />
                  <span className="font-medium">{i.harness_type}</span>
                  <span className="text-xs text-slate-400">
                    v{i.version ?? "?"} · {i.status}
                  </span>
                </label>
              </li>
            ))}
          </ul>
          <button
            onClick={() => setStep("review")}
            disabled={selected.length === 0}
            className="mt-4 rounded bg-blue-600 px-4 py-2 text-white disabled:opacity-50"
          >
            Review ({selected.length}) →
          </button>
        </section>
      )}

      {step === "review" && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">Review parsed state</h1>
          {reviewLoading && <p className="mt-2" role="status">Reading every selected harness…</p>}
          {reviewError && (
            <p className="mt-2 text-red-600">{String(reviewError)}</p>
          )}
          {reviewComplete && reviewed.length > 0 && !reviewLoading && (
            <>
              <div className="mt-4 grid grid-cols-3 gap-3 text-center">
                <Card label="Models" value={totalReview.models} />
                <Card label="MCP servers" value={totalReview.mcp} />
                <Card label="Skills" value={totalReview.skills} />
              </div>
              <div className="mt-3 rounded border border-slate-700 bg-slate-800 p-3 text-sm">
                <p className="font-medium text-slate-200">Complete import scope</p>
                <p className="mt-1 text-xs text-slate-400">{reviewed.length} harnesses · {totalReview.providers} providers · {totalReview.warnings} warnings</p>
                <div className="mt-3 flex flex-wrap gap-4">
                  <label><input type="checkbox" checked={includeModels} onChange={(e) => setIncludeModels(e.target.checked)} /> <span className="ml-1">Models</span></label>
                  <label><input type="checkbox" checked={includeMcp} onChange={(e) => setIncludeMcp(e.target.checked)} /> <span className="ml-1">MCP</span></label>
                  <label><input type="checkbox" checked={includeSkills} onChange={(e) => setIncludeSkills(e.target.checked)} /> <span className="ml-1">Skills</span></label>
                </div>
              </div>
              <div className="mt-4 space-y-2">
                {reviewed.map(({ installationId, state: harnessState }) => {
                  const harness = tier1.find((i) => i.id === installationId);
                  return <div key={installationId} className="rounded border border-slate-700 bg-slate-800 p-3 text-sm"><div className="font-medium text-slate-200">{harness?.harness_type ?? installationId}</div><div className="mt-1 text-xs text-slate-400">{harnessState.providers.length} providers · {harnessState.models.length} models · {harnessState.mcp.length} MCP · {harnessState.skills.length} skills</div>{harnessState.warnings.length > 0 && <div className="mt-1 text-xs text-amber-300">{harnessState.warnings.length} warning(s)</div>}</div>;
                })}
              </div>
              <div className="mt-4 text-xs text-slate-400">
                Import creates canonical entries in the local registry. It
                never writes to your harness config files.
              </div>
              {selected.length > 1 && (
                <label className="mt-3 block rounded border border-amber-500/30 bg-amber-500/10 p-3 text-xs text-amber-200">
                  <input type="checkbox" checked={independentCommitsAcknowledged} onChange={(e) => setIndependentCommitsAcknowledged(e.target.checked)} />
                  <span className="ml-2">I understand each harness is imported as a separate atomic transaction; if a later harness fails, earlier successful imports remain committed and are listed below.</span>
                </label>
              )}
              <button
                onClick={doImport}
                disabled={importing || (!includeModels && !includeMcp && !includeSkills) || (selected.length > 1 && !independentCommitsAcknowledged)}
                className="mt-4 rounded bg-blue-600 px-4 py-2 text-white disabled:opacity-50"
              >
                {importing ? "Importing…" : `Import from ${selected.length} harness(es)`}
              </button>
              {importError && (
                <p className="mt-2 text-red-600">Import failed: {importError}</p>
              )}
            </>
          )}
        </section>
      )}

      {step === "done" && reports.length > 0 && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">Import complete</h1>
          <ul className="mt-4 list-disc pl-5">
            <li>{total.providers} providers created</li>
            <li>{total.models} models imported</li>
            <li>{total.mcp} MCP servers imported</li>
            <li>{total.skills} skills imported</li>
            {total.symlinked > 0 && (
              <li>
                {total.symlinked} symlinked skills already canonical (no copy
                needed)
              </li>
            )}
          </ul>
          <div className="mt-4 rounded border border-slate-700 bg-slate-800 p-3 text-sm">
            <h2 className="font-medium">Per-harness results</h2>
            <ul className="mt-2 space-y-1 text-xs">
              {selected.map((installationId) => {
                const harness = tier1.find((i) => i.id === installationId);
                return <li key={installationId}>{harness?.harness_type ?? installationId}: {runStatus[installationId] ?? "skipped"}</li>;
              })}
            </ul>
          </div>
          {total.duplicates.length > 0 && (
            <div className="mt-4">
              <h2 className="font-medium">Skipped as duplicates</h2>
              <ul className="mt-1 list-disc pl-5 text-sm text-slate-300">
                {total.duplicates.map((d) => (
                  <li key={d}>{d}</li>
                ))}
              </ul>
            </div>
          )}
          {importError && (
            <p className="mt-2 text-red-600">Partial failure: {importError}</p>
          )}
          <Link
            to="/"
            className="mt-4 inline-block rounded bg-blue-600 px-4 py-2 text-white"
          >
            Open Dashboard
          </Link>
        </section>
      )}
    </div>
  );
}

function Card({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded border border-slate-700 bg-slate-800 p-4">
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-sm text-slate-300">{label}</div>
    </div>
  );
}
