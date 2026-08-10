import { type ReactNode } from "react";

export function SettingsRow({ title, description, control }: { title: string; description: string; control: ReactNode }) {
  return <div className="flex min-w-0 items-center justify-between gap-4"><div className="min-w-0"><p className="text-sm font-medium text-ink">{title}</p><p className="text-xs text-ink-muted">{description}</p></div><div className="shrink-0 whitespace-nowrap">{control}</div></div>;
}
