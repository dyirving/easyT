import { type ButtonHTMLAttributes, forwardRef } from "react";
import { cn } from "@/lib/utils";
import { Spinner } from "./Spinner";

export type ButtonVariant = "ghost" | "primary" | "outline" | "danger";
export type ButtonSize = "sm" | "md";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
  loadingLabel?: string;
}

const baseClass = "inline-flex items-center justify-center gap-1.5 rounded-control px-3 py-1.5 text-sm font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50";

const variantClass: Record<ButtonVariant, string> = {
  ghost: "text-ink-soft hover:bg-surface-soft hover:text-ink",
  primary: "bg-accent text-white hover:bg-accent/90",
  outline: "border border-line bg-surface-panel text-ink hover:bg-surface-soft",
  danger: "text-danger hover:bg-danger/10",
};

const sizeClass: Record<ButtonSize, string> = {
  sm: "h-8 px-2.5 text-xs",
  md: "h-9 px-3 text-sm",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "ghost", size = "md", loading = false, loadingLabel = "正在加载", disabled, children, ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(baseClass, variantClass[variant], sizeClass[size], className)}
        aria-busy={loading || undefined}
        disabled={disabled || loading}
        {...props}
      >
        {loading ? <Spinner label={loadingLabel} /> : null}
        {children}
      </button>
    );
  }
);
Button.displayName = "Button";
