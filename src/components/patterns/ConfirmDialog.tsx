import { Button, Dialog } from "@/components/ui";

export interface ConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  confirmLabel: string;
  cancelLabel: string;
  tone?: "default" | "danger";
  pending?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ConfirmDialog({ open, title, description, confirmLabel, cancelLabel, tone = "default", pending, onConfirm, onCancel }: ConfirmDialogProps) {
  return <Dialog open={open} onOpenChange={(next) => !next && onCancel()} title={title} description={description}><div className="flex justify-end gap-2"><Button variant="outline" onClick={onCancel} disabled={pending}>{cancelLabel}</Button><Button variant={tone === "danger" ? "danger" : "primary"} loading={pending} loadingLabel="正在确认" onClick={onConfirm}>{confirmLabel}</Button></div></Dialog>;
}
