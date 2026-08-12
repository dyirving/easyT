import { Children, cloneElement, forwardRef, isValidElement, type ButtonHTMLAttributes, type ReactElement } from "react";
import { cn } from "@/lib/utils";
import { Spinner } from "./Spinner";

type IconButtonVariant = "ghost" | "outline" | "danger";
type IconButtonSize = "sm" | "md";

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children" | "title"> {
  variant?: IconButtonVariant;
  size?: IconButtonSize;
  label: string;
  pressed?: boolean;
  loading?: boolean;
  children: ReactElement<{ "aria-hidden"?: boolean }>;
}

const baseClass = "inline-flex shrink-0 items-center justify-center rounded-control transition-colors disabled:cursor-not-allowed disabled:opacity-50";

const variantClass: Record<IconButtonVariant, string> = {
  ghost: "text-ink-soft hover:bg-surface-soft hover:text-ink",
  outline: "border border-line bg-surface-panel text-ink hover:bg-surface-soft",
  danger:
    "border border-danger/50 bg-surface-panel text-danger hover:border-danger hover:bg-danger/10",
};

const sizeClass: Record<IconButtonSize, string> = {
  sm: "h-8 w-8 [&>svg]:h-4 [&>svg]:w-4",
  md: "h-9 w-9 [&>svg]:h-4 [&>svg]:w-4",
};

/** A labelled, icon-only action. Its icon is always decorative. */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(
  ({ className, variant = "ghost", size = "md", label, pressed, loading = false, disabled, children, ...props }, ref) => {
    const icon = isValidElement(children) ? cloneElement(Children.only(children), { "aria-hidden": true }) : children;

    return (
      <button
        ref={ref}
        type="button"
        className={cn(baseClass, variantClass[variant], sizeClass[size], className)}
        aria-label={label}
        aria-pressed={pressed}
        aria-busy={loading || undefined}
        disabled={disabled || loading}
        title={label}
        {...props}
      >
        {loading ? <Spinner size="sm" /> : icon}
      </button>
    );
  },
);
IconButton.displayName = "IconButton";
