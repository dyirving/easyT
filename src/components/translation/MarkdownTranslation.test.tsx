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

  it("renders GFM tables with semantic cells and inline formatting", () => {
    const table = `| Models | AUC | BS |
| :--- | ---: | ---: |
| ProSeNet | 0.5333 | 0.2500 |
| DeepMoji | 0.7385 | 0.1935 |
| BiLSTM | 0.7436 | 0.1859 |
| ON-LSTM | 0.7487 | 0.2001 |
| Transformer | 0.7538 | 0.2009 |
| Ours w/o Rally-level input | 0.8649 | 0.1476 |
| Ours | **0.8966** | **0.1329** |`;
    const { container } = render(<MarkdownTranslation text={table} />);

    expect(container.querySelector("table")).toBeInTheDocument();
    expect(container.querySelectorAll("thead th")).toHaveLength(3);
    expect(container.querySelectorAll("tbody tr")).toHaveLength(7);
    expect(container.querySelector("tbody td strong")).toHaveTextContent("0.8966");
    expect(
      (container.querySelector("thead th:nth-child(2)") as HTMLElement).style
        .textAlign,
    ).toBe("right");
  });
});
