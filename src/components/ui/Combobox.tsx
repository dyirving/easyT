import {
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
  type HTMLAttributes,
  type KeyboardEvent,
} from "react";
import { ChevronDown } from "lucide-react";
import { cn } from "@/lib/utils";
import { useFormControlContext } from "./FormField";

export interface ComboboxOption {
  value: string;
  label: string;
}

export interface ComboboxProps {
  value: string;
  options: ComboboxOption[];
  onValueChange(value: string): void;
  placeholder?: string;
  disabled?: boolean;
  required?: boolean;
  inputMode?: HTMLAttributes<HTMLInputElement>["inputMode"];
}

export function Combobox({
  value,
  options,
  onValueChange,
  placeholder,
  disabled,
  required,
  inputMode,
}: ComboboxProps) {
  const field = useFormControlContext();
  const generatedId = useId();
  const inputId = field?.id ?? generatedId;
  const listId = `${inputId}-listbox`;
  const rootRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [highlighted, setHighlighted] = useState(0);
  const [queryDirty, setQueryDirty] = useState(false);
  const filtered = useMemo(() => {
    if (!queryDirty) return options;
    const query = value.trim().toLocaleLowerCase();
    return query
      ? options.filter(
          (option) =>
            option.value.toLocaleLowerCase().includes(query) ||
            option.label.toLocaleLowerCase().includes(query),
        )
      : options;
  }, [options, queryDirty, value]);

  useEffect(() => {
    setHighlighted(0);
  }, [value]);

  useEffect(() => {
    if (!open) return;
    const closeOutside = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", closeOutside);
    return () => document.removeEventListener("pointerdown", closeOutside);
  }, [open]);

  const choose = (option: ComboboxOption) => {
    onValueChange(option.value);
    setQueryDirty(false);
    setOpen(false);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      setOpen(false);
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      const direction = event.key === "ArrowDown" ? 1 : -1;
      setHighlighted((current) =>
        filtered.length
          ? (current + direction + filtered.length) % filtered.length
          : 0,
      );
      return;
    }
    if (event.key === "Enter" && open && filtered[highlighted]) {
      event.preventDefault();
      choose(filtered[highlighted]);
    }
  };

  return (
    <div ref={rootRef} className="relative">
      <div className="relative">
        <input
          id={inputId}
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={listId}
          aria-activedescendant={
            open && filtered[highlighted]
              ? `${listId}-${highlighted}`
              : undefined
          }
          aria-describedby={field?.describedBy}
          aria-invalid={field?.invalid || undefined}
          aria-required={(required ?? field?.required) || undefined}
          required={required ?? field?.required}
          disabled={disabled}
          inputMode={inputMode}
          value={value}
          placeholder={placeholder}
          onFocus={() => {
            setQueryDirty(false);
            setOpen(true);
          }}
          onClick={() => {
            setQueryDirty(false);
            setOpen(true);
          }}
          onChange={(event) => {
            setQueryDirty(true);
            onValueChange(event.target.value);
            setOpen(true);
          }}
          onKeyDown={onKeyDown}
          className="min-h-[var(--input-min-height)] w-full rounded-control border border-line bg-surface-panel px-3 py-2 pr-9 text-sm text-ink outline-none transition placeholder:text-ink-muted focus:border-accent focus:ring-1 focus:ring-accent/30 disabled:cursor-not-allowed disabled:opacity-50"
        />
        <ChevronDown
          aria-hidden="true"
          className="pointer-events-none absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 text-ink-muted"
        />
      </div>
      {open && !disabled ? (
        <ul
          id={listId}
          role="listbox"
          className="absolute z-20 mt-1 max-h-36 w-full overflow-y-auto rounded-control border border-line bg-surface-panel p-1 shadow-soft"
        >
          {filtered.length ? (
            filtered.map((option, index) => (
              <li
                id={`${listId}-${index}`}
                key={option.value}
                role="option"
                aria-selected={option.value === value}
                onPointerMove={() => setHighlighted(index)}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => choose(option)}
                className={cn(
                  "cursor-pointer rounded-compact px-2 py-1.5 text-sm text-ink",
                  index === highlighted && "bg-surface-soft",
                  option.value === value && "font-medium text-accent",
                )}
              >
                {option.label}
              </li>
            ))
          ) : (
            <li className="px-2 py-1.5 text-xs text-ink-muted">
              可直接使用当前输入值
            </li>
          )}
        </ul>
      ) : null}
    </div>
  );
}
