import { Button, Collapsible, Textarea } from "@/components/ui";

interface ManualTranslationInputProps {
  open: boolean;
  value: string;
  maxLength: number;
  disabled: boolean;
  onOpenChange(open: boolean): void;
  onValueChange(value: string): void;
  onTranslate(): void;
}

export function ManualTranslationInput({
  open,
  value,
  maxLength,
  disabled,
  onOpenChange,
  onValueChange,
  onTranslate,
}: ManualTranslationInputProps) {
  return (
    <Collapsible
      open={open}
      onOpenChange={onOpenChange}
      title="手动输入文本"
      summary={value ? `${value.slice(0, 80)}${value.length > 80 ? "…" : ""}` : ""}
      disabled={disabled}
    >
      <Textarea
        value={value}
        onChange={(event) => onValueChange(event.target.value)}
        placeholder="例如：Large language models are trained on massive text corpora."
        rows={3}
        disabled={disabled}
        className="resize-none leading-relaxed"
      />
      <div className="mt-2 flex items-center justify-between gap-2">
        <span className="text-xs text-ink-muted">
          {value.length} / {maxLength}
        </span>
        <Button
          variant="primary"
          size="sm"
          onClick={onTranslate}
          disabled={disabled || !value.trim() || value.length > maxLength}
        >
          翻译
        </Button>
      </div>
    </Collapsible>
  );
}
