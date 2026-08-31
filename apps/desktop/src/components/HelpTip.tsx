import { useEffect, useId, useState } from "react";

type HelpTipProps = {
  /** Short accessible name for the help trigger. */
  label: string;
  children: React.ReactNode;
  side?: "top" | "right" | "bottom" | "left";
};

/**
 * A compact, keyboard-accessible help affordance for controls whose meaning
 * is not obvious from the label alone. It opens on hover, focus, or click and
 * stays local to the control instead of adding a persistent help panel.
 */
export function HelpTip({ label, children, side = "top" }: HelpTipProps) {
  const [open, setOpen] = useState(false);
  const id = useId();

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [open]);

  const position = {
    top: "bottom-full left-1/2 mb-2 -translate-x-1/2",
    right: "left-full top-1/2 ml-2 -translate-y-1/2",
    bottom: "left-1/2 top-full mt-2 -translate-x-1/2",
    left: "right-full top-1/2 mr-2 -translate-y-1/2",
  }[side];

  return (
    <span
      className="relative inline-flex shrink-0 align-middle"
      onMouseEnter={() => setOpen(true)}
      onMouseLeave={() => setOpen(false)}
    >
      <button
        type="button"
        aria-label={`More information: ${label}`}
        aria-describedby={id}
        aria-expanded={open}
              onFocus={() => setOpen(true)}
              onBlur={(event) => {
                if (
                  !(event.relatedTarget instanceof Node) ||
                  !event.currentTarget.parentElement?.contains(event.relatedTarget)
                ) {
                  setOpen(false);
                }
              }}
        onClick={() => setOpen((value) => !value)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.stopPropagation();
            setOpen(false);
          }
        }}
        className="inline-flex h-4 w-4 items-center justify-center border border-slate-600 text-[10px] font-semibold leading-none text-slate-400 hover:border-slate-400 hover:text-slate-200"
      >
        ?
      </button>
      <span
        id={id}
        role="tooltip"
        aria-hidden={!open}
        className={`pointer-events-none absolute z-50 w-max max-w-xs whitespace-normal border border-slate-600 bg-slate-950 px-2.5 py-1.5 text-left text-xs font-normal leading-relaxed text-slate-200 shadow-xl transition-opacity ${position} ${open ? "opacity-100" : "opacity-0"}`}
      >
        {children}
      </span>
    </span>
  );
}
