import { type ButtonHTMLAttributes, forwardRef } from "react";
import { cn } from "@/lib/utils";

type Variant = "ghost" | "primary" | "outline" | "danger";
type Size = "sm" | "md" | "icon";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
}

const variantClass: Record<Variant, string> = {
  ghost: "btn btn-ghost",
  primary: "btn btn-primary",
  outline: "btn btn-outline",
  danger: "btn btn-danger",
};

const sizeClass: Record<Size, string> = {
  sm: "h-8 px-2.5 text-xs",
  md: "h-9 px-3 text-sm",
  icon: "h-8 w-8 p-0",
};

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant = "ghost", size = "md", ...props }, ref) => {
    return (
      <button
        ref={ref}
        className={cn(variantClass[variant], sizeClass[size], className)}
        {...props}
      />
    );
  }
);
Button.displayName = "Button";
