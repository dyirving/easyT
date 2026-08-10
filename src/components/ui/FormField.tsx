import { createContext, isValidElement, useContext, useId, type ReactNode } from "react";
import { cn } from "@/lib/utils";

type FormControlContextValue = { id: string; describedBy?: string; invalid?: boolean; required?: boolean };
const FormControlContext = createContext<FormControlContextValue | null>(null);
export const useFormControlContext = () => useContext(FormControlContext);

export interface FormFieldProps { label: string; hint?: string; error?: string; required?: boolean; children: ReactNode; className?: string }

export function FormField({ label, hint, error, required, children, className }: FormFieldProps) {
  const generatedId = useId();
  const childId = isValidElement<{ id?: string }>(children) ? children.props.id : undefined;
  const id = childId || generatedId;
  const hintId = hint ? `${id}-hint` : undefined;
  const errorId = error ? `${id}-error` : undefined;
  const describedBy = [hintId, errorId].filter(Boolean).join(" ") || undefined;

  return <FormControlContext.Provider value={{ id, describedBy, invalid: Boolean(error), required }}>
    <div className={cn("space-y-1.5", className)}>
      <label htmlFor={id} className="block text-sm font-medium text-ink">{label}{required ? <span aria-hidden="true"> *</span> : null}</label>
      {children}
      {error ? <p id={errorId} className="text-xs text-danger">{error}</p> : null}
      {hint ? <p id={hintId} className="text-xs text-ink-muted">{hint}</p> : null}
    </div>
  </FormControlContext.Provider>;
}
