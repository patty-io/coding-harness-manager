import { Link, useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { useDashboardStats } from "../hooks/useDashboard";
import {
  useInstallations,
  useHarnessDrift,
} from "../hooks/useHarnesses";
import { readHarnessState, type HarnessInstallation } from "../lib/api";
import { useState } from "react";
import { SyncDialog } from "../components/SyncDialog";

export function DashboardScreen() {
  const { data: stats } = useDashboardStats();
  const { data: installations, isLoading } = useInstallations();
  const [syncing, setSyncing] = useState<HarnessInstallation | null>(null);

  return (
    <div>
      <h1 className="text-2xl font-bold">Dashboard</h1>

      <div className="mt-4 grid grid-cols-2 gap-4 md:grid-cols-3 xl:grid-cols-6">
        <StatCard label="Harnesses" value={stats?.harnesses ?? "—"} to="/harnesses" />
        <StatCard label="Providers" value={stats?.providers ?? "—"} to="/providers" />
        <StatCard label="My Models" value={stats?.models ?? "—"} to="/models" />
        <StatCard label="MCP Servers" value={stats?.mcp ?? "—"} to="/mcp" />
        <StatCard label="Skills" value={stats?.skills ?? "—"} to="/skills" />
        <StatCard
          label="Drifted"
          value={stats?.drifted ?? "—"}
          alert={(stats?.drifted ?? 0) > 0}
          to="/harnesses"
        />
      </div>

      <div className="mt-8 flex items-center justify-between">
        <h2 className="font-medium text-slate-200">Harnesses</h2>
        <Link to="/harnesses" className="text-sm text-blue-400 hover:text-blue-300">
          All harnesses →
        </Link>
      </div>

      {isLoading && <p className="mt-3 text-sm text-slate-400">Loading…</p>}

      <div className="mt-3 grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
        {(installations ?? []).map((i) => (
          <HarnessCard
            key={i.id}
            installation={i}
            onReview={() => setSyncing(i)}
          />
        ))}
      </div>

      {(installations ?? []).length === 0 && !isLoading && (
        <div className="mt-4 rounded border border-dashed border-slate-700 bg-slate-800/40 p-6 text-center">
          <p className="text-sm text-slate-400">
            No harnesses detected yet — run a scan.
          </p>
          <Link
            to="/harnesses"
            className="mt-3 inline-block rounded bg-blue-600 px-4 py-2 text-sm text-white hover:bg-blue-500"
          >
            Scan machine
          </Link>
        </div>
      )}

      {syncing && (
        <SyncDialog
          installationId={syncing.id}
          harnessType={syncing.harness_type}
          onClose={() => setSyncing(null)}
        />
      )}
    </div>
  );
}

function HarnessCard({
  installation,
  onReview,
}: {
  installation: HarnessInstallation;
  onReview: () => void;
}) {
  const navigate = useNavigate();
  const { data: drift } = useHarnessDrift(installation.id);
  const { data: state } = useQuery({
    queryKey: ["harness-state", installation.id],
    queryFn: () => readHarnessState(installation.id),
    retry: false,
  });

  const statusStyles: Record<string, string> = {
    installed: "bg-green-500/15 text-green-400 border-green-500/30",
    detected: "bg-slate-700/50 text-slate-300 border-slate-600",
    "config-missing": "bg-amber-500/15 text-amber-400 border-amber-500/30",
    error: "bg-red-500/15 text-red-400 border-red-500/30",
  };
  const statusLabels: Record<string, string> = {
    installed: "Installed",
    detected: "Detected",
    "config-missing": "No binary",
    error: "Error",
  };

  return (
    <div
      onClick={() => navigate(`/harnesses/${installation.id}`)}
      className="cursor-pointer rounded-lg border border-slate-700 bg-slate-800 p-4 transition-colors hover:border-slate-500"
    >
      <div className="flex items-center justify-between">
        <span className="font-medium capitalize text-slate-100">
          {installation.harness_type}
        </span>
        <span
          className={`rounded border px-2 py-0.5 text-xs ${
            statusStyles[installation.status] ?? statusStyles.detected
          }`}
        >
          {statusLabels[installation.status] ?? installation.status}
        </span>
      </div>

      {drift?.drifted && (
        <p className="mt-2 rounded border border-amber-500/40 bg-amber-500/10 px-2 py-1 text-xs text-amber-300">
          Config changed outside the app
        </p>
      )}

      <div className="mt-3 flex gap-4 text-xs text-slate-400">
        <span>
          <strong className="text-slate-200">{state?.models.length ?? "…"}</strong>{" "}
          models
        </span>
        <span>
          <strong className="text-slate-200">{state?.mcp.length ?? "…"}</strong>{" "}
          mcp
        </span>
        <span>
          <strong className="text-slate-200">{state?.skills.length ?? "…"}</strong>{" "}
          skills
        </span>
      </div>

      <div className="mt-3 flex items-center justify-between border-t border-slate-700/60 pt-3">
        <span className="text-xs text-blue-400">Open →</span>
        <button
          onClick={(e) => {
            e.stopPropagation();
            onReview();
          }}
          className="rounded border border-blue-500/60 bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-300 hover:bg-blue-500/25"
        >
          Sync from library…
        </button>
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  alert,
  to,
}: {
  label: string;
  value: string | number;
  alert?: boolean;
  to: string;
}) {
  return (
    <Link
      to={to}
      className={`rounded border p-4 transition-colors ${
        alert
          ? "border-amber-500/40 bg-amber-500/10 hover:border-amber-400"
          : "border-slate-700 bg-slate-800 hover:border-slate-500"
      }`}
    >
      <div className="text-2xl font-bold">{value}</div>
      <div className="text-sm text-slate-300">{label}</div>
    </Link>
  );
}