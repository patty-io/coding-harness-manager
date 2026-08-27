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
  if (outcome === "Unknown") {
    return (
      <Modal onClose={onClose}>
        <h2 className="font-medium">No models.dev match</h2>
        <p className="mt-1 text-sm text-slate-300">
          No canonical model could be matched for this route. You can still use
          it; metadata will remain provider-discovered.
        </p>
        <CloseButton onClose={onClose} />
      </Modal>
    );
  }
  if ("Matched" in outcome) {
    return (
      <Modal onClose={onClose}>
        <h2 className="font-medium">
          Matched: {outcome.Matched.identity_name}
        </h2>
        <p className="mt-1 text-sm text-slate-300">
          {outcome.Matched.confidence}% confidence — linked automatically.
        </p>
        <CloseButton onClose={onClose} />
      </Modal>
    );
  }
  const candidates = outcome.Ambiguous.candidates;
  return (
    <Modal onClose={onClose}>
      <h2 className="font-medium">Ambiguous match — choose a candidate</h2>
      <p className="mt-1 text-sm text-slate-300">
        Multiple canonical models could correspond to this route. Select the
        right one:
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