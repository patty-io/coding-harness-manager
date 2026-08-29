import { useState } from "react";
import { announceToast } from "./Toast";

export function ConfirmDialog({
  title,
  message,
  confirmLabel = "Confirm",
  onConfirm,
  onClose,
}: {
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void | Promise<void>;
  onClose: () => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    if (pending) return;
    setPending(true);
    setError(null);
    try {
      await onConfirm();
      announceToast({ message: `${title} completed`, tone: "success", link: "/history" });
      onClose();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      setPending(false);
    }
  };
  return (
    <div
      className="fixed inset-0 z-30 flex items-center justify-center bg-black/60"
      role="presentation"
      onClick={() => !pending && onClose()}
    >
      <div
        className="w-[24rem] rounded border border-slate-700 bg-slate-800 p-4"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="font-medium text-slate-100" role="heading">{title}</h3>
        <p className="mt-2 text-sm leading-relaxed text-slate-400">{message}</p>
        {error && <p className="mt-3 rounded bg-red-950/70 p-2 text-sm text-red-200" role="alert">{error}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            disabled={pending}
            className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
          >
            Cancel
          </button>
          <button
            onClick={() => void submit()}
            disabled={pending}
            className="rounded bg-red-600 px-3 py-1 text-sm text-white hover:bg-red-500"
          >
            {pending ? `${confirmLabel}…` : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}

type ConfirmState = {
  title: string;
  message: string;
  confirmLabel?: string;
  onConfirm: () => void | Promise<void>;
};

/**
 * window.confirm() does not render inside the Tauri webview, so every
 * destructive action routes through this in-app dialog instead.
 */
export function useConfirm() {
  const [state, setState] = useState<ConfirmState | null>(null);
  const confirm = (
    title: string,
    message: string,
    onConfirm: () => void,
    confirmLabel?: string,
  ) => setState({ title, message, onConfirm, confirmLabel });
  const confirmDialog = state ? (
    <ConfirmDialog
      title={state.title}
      message={state.message}
      confirmLabel={state.confirmLabel}
      onConfirm={state.onConfirm}
      onClose={() => setState(null)}
    />
  ) : null;
  return { confirm, confirmDialog };
}
