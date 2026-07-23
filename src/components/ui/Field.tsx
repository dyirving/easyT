import { type HTMLAttributes } from "react";
import { cn } from "@/lib/utils";

interface FieldProps extends HTMLAttributes<HTMLDivElement> {
  label: string;
  hint?: string;
  htmlFor?: string;
}

/** 设置页字段：标签 + 描述 + 控件 */
export function Field({
  label,
  hint,
  htmlFor,
  className,
  children,
  ...props
}: FieldProps) {
  return (
    <div className={cn("space-y-1.5", className)} {...props}>
      <label
        htmlFor={htmlFor}
        className="block text-sm font-medium text-ink"
      >
        {label}
      </label>
      {children}
      {hint ? <p className="text-xs text-ink-muted">{hint}</p> : null}
    </div>
  );
}
