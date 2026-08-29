import { useEffect, useState } from "react";
import { Link } from "react-router-dom";

export type ToastPayload = {
  message: string;
  tone?: "success" | "error" | "info";
  link?: string;
};

export function announceToast(payload: ToastPayload) {
  window.dispatchEvent(new CustomEvent<ToastPayload>("chm-toast", { detail: payload }));
}

export function ToastViewport() {
  const [toasts, setToasts] = useState<Array<ToastPayload & { id: number }>>([]);
  useEffect(() => {
    const onToast = (event: Event) => {
      const payload = (event as CustomEvent<ToastPayload>).detail;
      const id = Date.now() + Math.random();
      setToasts((current) => [...current, { ...payload, id }].slice(-4));
      window.setTimeout(() => setToasts((current) => current.filter((toast) => toast.id !== id)), 7000);
    };
    window.addEventListener("chm-toast", onToast);
    return () => window.removeEventListener("chm-toast", onToast);
  }, []);
  return (
    <div className="pointer-events-none fixed right-4 top-4 z-[80] flex w-80 flex-col gap-2" aria-live="polite" aria-atomic="true">
      {toasts.map((toast) => (
        <div key={toast.id} className={`pointer-events-auto rounded border p-3 text-sm shadow-lg ${toast.tone === "error" ? "border-red-500/50 bg-red-950 text-red-200" : toast.tone === "info" ? "border-slate-600 bg-slate-800 text-slate-200" : "border-green-500/50 bg-green-950 text-green-200"}`}>
          <div>{toast.message}</div>
          {toast.link && <Link to={toast.link} className="mt-1 inline-block underline">View details</Link>}
        </div>
      ))}
    </div>
  );
}
