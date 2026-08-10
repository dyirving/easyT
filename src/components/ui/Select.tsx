import { forwardRef, type SelectHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { useFormControlContext } from "./FormField";
export const Select = forwardRef<HTMLSelectElement, SelectHTMLAttributes<HTMLSelectElement>>(({ className, id, "aria-describedby": describedBy, "aria-invalid": invalid, required, ...props }, ref) => {
  const field = useFormControlContext();
  return <select ref={ref} id={id ?? field?.id} aria-describedby={[describedBy, field?.describedBy].filter(Boolean).join(" ") || undefined} aria-invalid={(invalid ?? field?.invalid) || undefined} aria-required={(required ?? field?.required) || undefined} required={required ?? field?.required} className={cn("min-h-[var(--input-min-height)] w-full rounded-control border border-line bg-surface-panel px-3 py-2 text-sm text-ink outline-none transition focus:border-accent focus:ring-1 focus:ring-accent/30 disabled:cursor-not-allowed disabled:opacity-50", className)} {...props} />;
});
Select.displayName = "Select";
