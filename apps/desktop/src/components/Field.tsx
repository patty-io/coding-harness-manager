import { cloneElement, isValidElement, type ReactNode } from "react";

type FieldProps = {
  id: string;
  label: string;
  description?: string;
  error?: string | null;
  required?: boolean;
  children: ReactNode;
};

/** Label, description, and validation wiring shared by all forms. */
export function Field({
  id,
  label,
  description,
  error,
  required,
  children,
}: FieldProps) {
  const descriptionId = description ? `${id}-description` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [descriptionId, errorId].filter(Boolean).join(" ") || undefined;
  return (
    <div>
      <label htmlFor={id} className="block text-xs text-slate-500">
        {label}
        {required && <span className="ml-1 text-red-300" aria-hidden="true">*</span>}
      </label>
      {description && (
        <p id={descriptionId} className="mt-0.5 text-[11px] text-slate-500">
          {description}
        </p>
      )}
      <div className="mt-1">
        {isValidElement(children)
          ? cloneElement(children, {
              id,
              "aria-describedby": describedBy,
              "aria-invalid": error ? true : undefined,
            } as Record<string, unknown>)
          : children}
      </div>
      {error && <FormError id={errorId!}>{error}</FormError>}
    </div>
  );
}

export function FormError({ id, children }: { id: string; children: ReactNode }) {
  return (
    <p id={id} role="alert" className="mt-1 text-xs text-red-400">
      {children}
    </p>
  );
}

/** Props helper for wiring Field metadata onto a native control. */
export function fieldA11y(id: string, description?: string, error?: string | null) {
  const describedBy = [description && `${id}-description`, error && `${id}-error`]
    .filter(Boolean)
    .join(" ");
  return {
    id,
    "aria-describedby": describedBy || undefined,
    "aria-invalid": error ? (true as const) : undefined,
  };
}
