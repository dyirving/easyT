import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { useFormControlContext } from "./FormField";

export interface SwitchProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "onChange"> {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  disabled?: boolean;
  required?: boolean;
  "aria-label"?: string;
}

/** 极简开关（shadcn/ui 风格的最小实现，避免引入额外依赖） */
export const Switch = forwardRef<HTMLButtonElement, SwitchProps>(({ 
  checked,
  onCheckedChange,
  disabled,
  "aria-describedby": describedBy,
  "aria-invalid": invalid,
  required,
  ...props
}, ref) => {
  const field = useFormControlContext();
  return (
    <button
      ref={ref} type="button"
      role="switch"
      aria-checked={checked}
      aria-describedby={[describedBy, field?.describedBy].filter(Boolean).join(" ") || undefined}
      aria-invalid={(invalid ?? field?.invalid) || undefined}
      aria-required={(required ?? field?.required) || undefined}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors disabled:cursor-not-allowed disabled:opacity-50",
        checked ? "bg-accent" : "bg-line"
      )}
      {...props}
    >
      <span
        className={cn(
          "inline-block h-4 w-4 transform rounded-full bg-white shadow transition-transform",
          checked ? "translate-x-4" : "translate-x-0.5"
        )}
      />
    </button>
  );
});
Switch.displayName = "Switch";
