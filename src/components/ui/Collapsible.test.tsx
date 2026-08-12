import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { Collapsible } from "./index";

describe("Collapsible", () => {
  it("is controlled and exposes its content relationship", () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <Collapsible
        open={false}
        onOpenChange={onOpenChange}
        title="原文"
        summary="summary"
      >
        <p>full body</p>
      </Collapsible>,
    );
    const trigger = screen.getByRole("button", { name: /原文/ });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).toHaveAttribute("aria-controls");
    expect(screen.getByText("summary")).toBeVisible();
    expect(screen.getByText("full body")).not.toBeVisible();
    fireEvent.click(trigger);
    expect(onOpenChange).toHaveBeenCalledWith(true);

    rerender(
      <Collapsible open onOpenChange={onOpenChange} title="原文" summary="summary">
        <p>full body</p>
      </Collapsible>,
    );
    expect(screen.getByText("full body")).toBeVisible();
    expect(screen.queryByText("summary")).not.toBeInTheDocument();
  });

  it("unmounts closed content when requested and respects disabled", () => {
    const onOpenChange = vi.fn();
    render(
      <Collapsible
        open={false}
        onOpenChange={onOpenChange}
        title="译文"
        disabled
        unmountOnClose
      >
        <p>markdown tree</p>
      </Collapsible>,
    );
    const trigger = screen.getByRole("button", { name: "译文" });
    expect(trigger).toBeDisabled();
    expect(screen.queryByText("markdown tree")).not.toBeInTheDocument();
    fireEvent.click(trigger);
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
