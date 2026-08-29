import { useEffect, useId, useRef, useState } from "react";
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
  const titleId = useId();
  const messageId = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const confirmRef = useRef<HTMLButtonElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);
  const pendingRef = useRef(false);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    previousFocus.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    confirmRef.current?.focus();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !pendingRef.current) {
        event.preventDefault();
        onClose();
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("keydown", onKeyDown);
      previousFocus.current?.focus();
    };
  }, [onClose]);
  const submit = async () => {
    if (pending) return;
    pendingRef.current = true;
    setPending(true);
    setError(null);
    try {
      await onConfirm();
      announceToast({ message: `${title} completed`, tone: "success", link: "/history" });
      onClose();
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
    } finally {
      pendingRef.current = false;
      setPending(false);
    }
  };
  const keepFocusInside = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab") return;
    const focusable = dialogRef.current?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled])',
    );
    if (!focusable || focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
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
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={messageId}
        tabIndex={-1}
        onKeyDown={keepFocusInside}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 id={titleId} className="font-medium text-slate-100">{title}</h3>
        <p id={messageId} className="mt-2 text-sm leading-relaxed text-slate-400">{message}</p>
        {error && <p className="mt-3 rounded bg-red-950/70 p-2 text-sm text-red-200" role="alert">{error}</p>}
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onClose}
            disabled={pending}
            className="rounded px-3 py-1 text-sm text-slate-400 hover:text-slate-200"
          >
            Cancel
          </button>
          <button
            ref={confirmRef}
            type="button"
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
    onConfirm: () => void | Promise<void>,
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
