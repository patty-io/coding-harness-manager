import { useState } from "react";

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
  onConfirm: () => void;
  onClose: () => void;
}) {
  return (
    <div
      className="fixed inset-0 z-30 flex items-center justify-center bg-black/60"
      onClick={onClose}
    >
      <div
        className="w-[24rem] rounded border border-slate-700 bg-slate-800 p-4"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="font-medium text-slate-100">{title}</h3>
        <p className="mt-2 text-sm leading-relaxed text-slate-400">{message}</p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            onClick={onClose}
            className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
          >
            Cancel
          </button>
          <button
            onClick={() => {
              onConfirm();
              onClose();
            }}
            className="rounded bg-red-600 px-3 py-1 text-sm text-white hover:bg-red-500"
          >
            {confirmLabel}
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
  onConfirm: () => void;
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