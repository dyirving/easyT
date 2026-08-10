import { createRef } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { Circle } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { Button, IconButton, Spinner } from "./index";

describe("Button", () => {
  it("keeps native props, variants, and ref behavior", () => {
    const onClick = vi.fn();
    const ref = createRef<HTMLButtonElement>();

    render(
      <Button ref={ref} variant="primary" size="sm" onClick={onClick} name="save">
        保存
      </Button>,
    );

    const button = screen.getByRole("button", { name: /保存/ });
    fireEvent.click(button);

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(button).toHaveAttribute("name", "save");
    expect(ref.current).toBe(button);
  });

  it("prevents repeat submission and exposes its loading label", () => {
    const onClick = vi.fn();

    render(
      <Button loading loadingLabel="正在保存" onClick={onClick}>
        保存
      </Button>,
    );

    const button = screen.getByRole("button", { name: /保存/ });
    fireEvent.click(button);

    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(onClick).not.toHaveBeenCalled();
    expect(screen.getByRole("status", { name: "正在保存" })).toBeInTheDocument();
  });
});

describe("IconButton", () => {
  it("uses its required label as the accessible name and hides its icon", () => {
    const onClick = vi.fn();
    const ref = createRef<HTMLButtonElement>();

    render(
      <IconButton ref={ref} label="固定窗口" pressed size="sm" onClick={onClick}>
        <Circle />
      </IconButton>,
    );

    const button = screen.getByRole("button", { name: "固定窗口" });
    fireEvent.click(button);

    expect(onClick).toHaveBeenCalledTimes(1);
    expect(button).toHaveAttribute("aria-pressed", "true");
    expect(button).toHaveAttribute("title", "固定窗口");
    expect(button.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(ref.current).toBe(button);
  });

  it("disables itself while loading without creating a second live announcement", () => {
    render(
      <IconButton label="重新翻译" loading>
        <Circle />
      </IconButton>,
    );

    const button = screen.getByRole("button", { name: "重新翻译" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("aria-busy", "true");
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});

describe("Spinner", () => {
  it("is decorative without a label and exposes a named status with one", () => {
    const { rerender } = render(<Spinner size="sm" />);

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(document.querySelector("[aria-hidden='true']")).toBeInTheDocument();

    rerender(<Spinner size="md" label="正在加载" />);

    expect(screen.getByRole("status", { name: "正在加载" })).toBeInTheDocument();
  });
});
