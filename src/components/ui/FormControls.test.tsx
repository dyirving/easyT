import { createRef } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import { FormField, Input, Select, Switch, Textarea } from "./index";

it("associates label, hint, error, required and caller descriptions", () => {
  render(<FormField label="API Key" hint="安全保存" error="必填" required><Input id="api-key" aria-describedby="external" /></FormField>);
  const input = screen.getByLabelText(/API Key/);
  expect(input).toHaveAttribute("id", "api-key");
  expect(input).toHaveAttribute("aria-describedby", expect.stringContaining("external"));
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(input).toBeRequired();
});

it("keeps controls usable without FormField and forwards refs", () => {
  const ref = createRef<HTMLTextAreaElement>();
  const change = vi.fn();
  render(<><Textarea ref={ref} aria-label="原文" /><Select aria-label="语言"><option>中文</option></Select><Switch aria-label="开关" checked={false} onCheckedChange={change} /></>);
  expect(ref.current).toBe(screen.getByLabelText("原文"));
  fireEvent.click(screen.getByRole("switch", { name: "开关" }));
  expect(change).toHaveBeenCalledWith(true);
});
