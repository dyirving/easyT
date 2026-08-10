import { type InputHTMLAttributes, forwardRef } from "react";
import { cn } from "@/lib/utils";
import { useFormControlContext } from "./FormField";

export interface InputProps extends InputHTMLAttributes<HTMLInputElement> {}

export const Input = forwardRef<HTMLInputElement, InputProps>(
  ({ className, id, "aria-describedby": describedBy, "aria-invalid": invalid, required, ...props }, ref) => {
    const field = useFormControlContext();
    return <input ref={ref} id={id ?? field?.id} aria-describedby={[describedBy, field?.describedBy].filter(Boolean).join(" ") || undefined} aria-invalid={(invalid ?? field?.invalid) || undefined} aria-required={(required ?? field?.required) || undefined} required={required ?? field?.required} className={cn("min-h-[var(--input-min-height)] w-full rounded-control border border-line bg-surface-panel px-3 py-2 text-sm text-ink outline-none transition placeholder:text-ink-muted focus:border-accent focus:ring-1 focus:ring-accent/30 disabled:cursor-not-allowed disabled:opacity-50", className)} {...props} />;
  }
);
Input.displayName = "Input";
