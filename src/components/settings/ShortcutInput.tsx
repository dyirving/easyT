import { useEffect, useRef, useState } from "react";
import { Keyboard } from "lucide-react";
import { Input } from "@/components/ui";

interface ShortcutInputProps {
  value: string;
  onChange: (shortcut: string) => void;
  disabled?: boolean;
}

/** 快捷键输入框：捕获按键并格式化为 "Ctrl+T" 形式 */
const MODIFIER_KEYS = new Set([
  "Control",
  "Shift",
  "Alt",
  "Meta",
  "ControlLeft",
  "ControlRight",
  "ShiftLeft",
  "ShiftRight",
  "AltLeft",
  "AltRight",
  "MetaLeft",
  "MetaRight",
]);

function keyToLabel(key: string): string {
  const map: Record<string, string> = {
    Control: "Ctrl",
    Meta: "Win",
    ArrowUp: "Up",
    ArrowDown: "Down",
    ArrowLeft: "Left",
    ArrowRight: "Right",
    " ": "Space",
  };
  return map[key] ?? key;
}

export function ShortcutInput({
  value,
  onChange,
  disabled,
}: ShortcutInputProps) {
  const [recording, setRecording] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  // Esc 取消录制
  useEffect(() => {
    if (!recording) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      // 忽略单独的修饰键
      if (MODIFIER_KEYS.has(e.key)) return;
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      if (e.metaKey) parts.push("Win");
      parts.push(keyToLabel(e.key.length === 1 ? e.key.toUpperCase() : e.key));
      onChange(parts.join("+"));
      setRecording(false);
    };
    window.addEventListener("keydown", handler, true);
    return () => window.removeEventListener("keydown", handler, true);
  }, [recording, onChange]);

  return (
    <div className="relative">
      <Keyboard
        aria-hidden="true"
        className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-muted"
      />
      <Input
        ref={inputRef}
        value={value}
        readOnly
        disabled={disabled}
        onFocus={() => setRecording(true)}
        onBlur={() => setRecording(false)}
        placeholder="点击并按下快捷键"
        className="cursor-pointer pl-9 pr-24"
      />
      {recording ? (
        <span className="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-accent">
          按下快捷键…
        </span>
      ) : null}
    </div>
  );
}
