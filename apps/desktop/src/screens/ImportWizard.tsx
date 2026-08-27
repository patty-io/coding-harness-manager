import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { TIER1_HARNESSES } from "../lib/api";
import { useInstallations, useScanHarnesses } from "../hooks/useHarnesses";
import { useImportHarnessState, useReadHarnessState } from "../hooks/useImport";
import type { ImportOptions, ImportReport } from "../lib/importApi";

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
  const { data: installations, isLoading } = useInstallations();
  const scan = useScanHarnesses();
  const active = selected[0] ?? null;
  const review = useReadHarnessState(active);
  const importMutation = useImportHarnessState();

  const tier1 = (installations ?? []).filter((i) =>
    TIER1_HARNESSES.includes(i.harness_type as (typeof TIER1_HARNESSES)[number]),
  );

  const stepIndex = STEPS.indexOf(step);

  const toggle = (id: string) =>
    setSelected((prev) =>
      prev.includes(id) ? prev.filter((x) => x !== id) : [...prev, id],
    );

  const doImport = async () => {
    setImporting(true);
    setImportError(null);
    const options: ImportOptions = {
      importModels: true,
      importMcp: true,
      importSkills: true,
    };
    const results = [];
    for (const id of selected) {
      try {
        results.push(await importMutation.mutateAsync({ installationId: id, options }));
      } catch (e) {
        setImportError(e instanceof Error ? e.message : String(e));
        break;
      }
    }
    setReports(results);
    setImporting(false);
    if (results.length > 0) setStep("done");
  };

  useEffect(() => {
    if (step === "review" && importMutation.isSuccess) setStep("done");
  }, [importMutation.isSuccess, step]);

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
      <ol className="flex gap-2 text-xs text-gray-500">
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
          <p className="mt-2 text-gray-700">
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
          <p className="mt-2 text-gray-700">
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
          {isLoading && <p className="mt-2 text-gray-500">Loading…</p>}
        </section>
      )}

      {step === "select" && (
        <section className="mt-6">
          <h1 className="text-2xl font-bold">
            Found {tier1.length} supported harnesses
          </h1>
          <p className="mt-2 text-gray-700">
            Select which harnesses to import from (read-only).
          </p>
          <ul className="mt-4 space-y-2">
            {tier1.map((i) => (
              <li key={i.id}>
                <label className="flex items-center gap-2 rounded border border-gray-200 bg-white p-3">
                  <input
                    type="checkbox"
                    checked={selected.includes(i.id)}
                    onChange={() => toggle(i.id)}
                  />
                  <span className="font-medium">{i.harness_type}</span>
                  <span className="text-xs text-gray-500">
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
          {review.isLoading && <p className="mt-2">Reading…</p>}
          {review.isError && (
            <p className="mt-2 text-red-600">{review.error.message}</p>
          )}
          {review.data && (
            <>
              {review.data.warnings.length > 0 && (
                <ul className="mt-2 list-disc pl-5 text-amber-700">
                  {review.data.warnings.map((w) => (
                    <li key={w}>{w}</li>
                  ))}
                </ul>
              )}
              <div className="mt-4 grid grid-cols-3 gap-3 text-center">
                <Card label="Models" value={review.data.models.length} />
                <Card label="MCP servers" value={review.data.mcp.length} />
                <Card label="Skills" value={review.data.skills.length} />
              </div>
              <div className="mt-4 text-xs text-gray-500">
                Import creates canonical entries in the local registry. It
                never writes to your harness config files.
              </div>
              <button
                onClick={doImport}
                disabled={importing}
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
          {total.duplicates.length > 0 && (
            <div className="mt-4">
              <h2 className="font-medium">Skipped as duplicates</h2>
              <ul className="mt-1 list-disc pl-5 text-sm text-gray-600">
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
    <div className="rounded border border-gray-200 bg-white p-4">
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-sm text-gray-600">{label}</div>
    </div>
  );
}