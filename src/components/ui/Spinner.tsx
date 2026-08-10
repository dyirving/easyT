import { Loader2 } from "lucide-react";
import { cn } from "@/lib/utils";

export interface SpinnerProps {
  size?: "sm" | "md";
  label?: string;
  className?: string;
}

const sizeClass: Record<NonNullable<SpinnerProps["size"]>, string> = {
  sm: "h-4 w-4",
  md: "h-5 w-5",
};

/** A status indicator that only announces itself when a label is supplied. */
export function Spinner({ size = "sm", label, className }: SpinnerProps) {
  const icon = <Loader2 aria-hidden="true" className={cn("animate-spin", sizeClass[size])} />;

  if (!label) {
    return <span aria-hidden="true" className={cn("inline-flex shrink-0", className)}>{icon}</span>;
  }

  return <span role="status" aria-label={label} className={cn("inline-flex shrink-0", className)}>{icon}</span>;
}
