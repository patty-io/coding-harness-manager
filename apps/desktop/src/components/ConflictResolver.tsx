import type { EnrichOutcome } from "../hooks/useModels";

export type { EnrichOutcome };

export function ConflictResolver({
  outcome,
  onResolve,
  onClose,
}: {
  outcome: EnrichOutcome;
  onResolve: (identityId: string) => void;
  onClose: () => void;
}) {
  if (outcome === "unknown") {
    return (
      <Modal onClose={onClose}>
        <h2 className="font-medium">No catalog match</h2>
        <p className="mt-1 text-sm text-slate-300">
          The model id did not match the local models.dev catalog. You can
          still use it; its current provider metadata is unchanged.
        </p>
        <CloseButton onClose={onClose} />
      </Modal>
    );
  }
  if ("matched" in outcome) {
    return (
      <Modal onClose={onClose}>
        <h2 className="font-medium">
          Catalog match: {outcome.matched.identity_name}
        </h2>
        <p className="mt-1 text-sm text-slate-300">
          {outcome.matched.confidence}% confidence — canonical metadata was
          linked automatically.
        </p>
        <CloseButton onClose={onClose} />
      </Modal>
    );
  }
  if (!("ambiguous" in outcome)) {
    return (
      <Modal onClose={onClose}>
        <h2 className="font-medium">Catalog match unavailable</h2>
        <p className="mt-1 text-sm text-slate-300">
          The matcher returned an unexpected result. Your model route was not
          changed.
        </p>
        <CloseButton onClose={onClose} />
      </Modal>
    );
  }
  const candidates = outcome.ambiguous.candidates;
  return (
    <Modal onClose={onClose}>
      <h2 className="font-medium">Choose catalog metadata</h2>
      <p className="mt-1 text-sm text-slate-300">
        More than one catalog model resembles this route. Choose the one that
        describes your provider model:
      </p>
      <ul className="mt-3 space-y-2">
        {candidates.map((c) => (
          <li key={c.modelsDevId}>
            <button
              onClick={() => onResolve(c.modelsDevId)}
              className="w-full rounded border border-slate-600 bg-slate-800 p-2 text-left text-sm hover:bg-blue-950"
            >
              <span className="font-medium">{c.displayName}</span>
              <span className="ml-2 font-mono text-xs text-slate-400">
                {c.modelsDevId}
              </span>
              <span className="ml-2 text-xs text-slate-400">
                context: {c.contextWindow?.toLocaleString() ?? "?"}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </Modal>
  );
}

function Modal({
  children,
  onClose,
}: {
  children: React.ReactNode;
  onClose: () => void;
}) {
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
      <div className="w-full max-w-md rounded bg-slate-800 p-4 shadow-xl">
        {children}
        <button
          onClick={onClose}
          className="mt-4 rounded border border-slate-600 px-3 py-1 text-sm"
        >
          Close
        </button>
      </div>
    </div>
  );
}

function CloseButton({ onClose }: { onClose: () => void }) {
  return (
    <button
      onClick={onClose}
      className="mt-4 rounded bg-blue-600 px-3 py-1 text-sm text-white"
    >
      OK
    </button>
  );
}
