import { useState } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Combobox, FormField } from "./index";

const options = [
  { value: "5", label: "5 条" },
  { value: "10", label: "10 条" },
  { value: "20", label: "20 条" },
];

function ControlledCombobox() {
  const [value, setValue] = useState("5");
  return (
    <FormField label="历史上限" hint="可输入 1～20" required>
      <Combobox value={value} options={options} onValueChange={setValue} />
    </FormField>
  );
}

describe("Combobox", () => {
  it("shows all presets on focus, filters typed input and selects a value", () => {
    render(<ControlledCombobox />);
    const input = screen.getByRole("combobox", { name: /历史上限/ });
    fireEvent.focus(input);
    expect(screen.getAllByRole("option")).toHaveLength(3);
    fireEvent.change(input, { target: { value: "10" } });
    expect(screen.getAllByRole("option")).toHaveLength(1);
    fireEvent.click(screen.getByRole("option", { name: "10 条" }));
    expect(input).toHaveValue("10");
    expect(input).toHaveAttribute("aria-describedby");
    expect(input).toBeRequired();
  });

  it("supports keyboard selection and escape", () => {
    render(<ControlledCombobox />);
    const input = screen.getByRole("combobox");
    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(input).toHaveValue("10");
    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "Escape" });
    expect(input).toHaveAttribute("aria-expanded", "false");
  });

  it("does not open while disabled", () => {
    const change = vi.fn();
    render(
      <Combobox
        value="5"
        options={options}
        onValueChange={change}
        disabled
      />,
    );
    const input = screen.getByRole("combobox");
    fireEvent.click(input);
    expect(input).toBeDisabled();
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
    expect(change).not.toHaveBeenCalled();
  });
});
