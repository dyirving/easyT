import { useId, type ReactNode } from "react";
import { ChevronDown, ChevronRight } from "lucide-react";

export interface CollapsibleProps {
  open: boolean;
  onOpenChange(open: boolean): void;
  title: ReactNode;
  summary?: ReactNode;
  children: ReactNode;
  disabled?: boolean;
  unmountOnClose?: boolean;
}

export function Collapsible({
  open,
  onOpenChange,
  title,
  summary,
  children,
  disabled,
  unmountOnClose = false,
}: CollapsibleProps) {
  const id = useId();
  const contentId = `${id}-content`;
  return (
    <div className="rounded-control border border-line bg-surface-panel">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={contentId}
        disabled={disabled}
        onClick={() => onOpenChange(!open)}
        className="flex min-h-9 w-full items-start gap-2 rounded-control px-3 py-2 text-left text-sm text-ink outline-none hover:bg-surface-soft focus-visible:ring-2 focus-visible:ring-accent/40 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {open ? (
          <ChevronDown aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0" />
        ) : (
          <ChevronRight aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0" />
        )}
        <span className="min-w-0 flex-1">
          <span className="block font-medium">{title}</span>
          {!open && summary ? (
            <span className="mt-1 block text-xs font-normal text-ink-muted">
              {summary}
            </span>
          ) : null}
        </span>
      </button>
      {open || !unmountOnClose ? (
        <div id={contentId} hidden={!open} className="border-t border-line p-3">
          {children}
        </div>
      ) : null}
    </div>
  );
}
