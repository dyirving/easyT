import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarkdownTranslation } from "./MarkdownTranslation";

describe("MarkdownTranslation", () => {
  it("renders a tagged equation without a KaTeX parse error", () => {
    const { container } = render(
      <MarkdownTranslation text={String.raw`$x = 1 \tag{1}$`} />,
    );

    expect(container.querySelector(".katex-error")).not.toBeInTheDocument();
    expect(container.querySelector(".katex-display")).toBeInTheDocument();
  });

  it("renders a same-line double-dollar tagged equation", () => {
    const { container } = render(
      <MarkdownTranslation text={String.raw`$$Y_{n,d} = X_{n,d} \tag{6}$$`} />,
    );

    expect(container.querySelector(".katex-error")).not.toBeInTheDocument();
    expect(container.querySelector(".katex-display")).toBeInTheDocument();
  });

  it("keeps ordinary inline math inline", () => {
    const { container } = render(
      <MarkdownTranslation text={String.raw`参数 $x = 1$ 保持行内。`} />,
    );

    expect(container.querySelector(".katex")).toBeInTheDocument();
    expect(container.querySelector(".katex-display")).not.toBeInTheDocument();
  });
});
