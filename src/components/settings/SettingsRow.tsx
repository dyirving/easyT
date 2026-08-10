import { type ReactNode } from "react";

export function SettingsRow({ title, description, control }: { title: string; description: string; control: ReactNode }) {
  return <div className="flex items-center justify-between gap-4"><div><p className="text-sm font-medium text-ink">{title}</p><p className="text-xs text-ink-muted">{description}</p></div>{control}</div>;
}
