import { ArrowLeft } from "lucide-react";
import { Button } from "@/components/ui";

export function SettingsHeader({ onBack }: { onBack: () => void }) {
  return <div className="flex items-center justify-between border-b border-line px-3 py-2"><Button variant="ghost" size="sm" onClick={onBack}><ArrowLeft className="h-4 w-4" />返回</Button><div className="flex flex-1 justify-center text-sm font-medium text-ink" data-tauri-drag-region>设置</div><span className="w-12" data-tauri-drag-region /></div>;
}
