import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TranslationPanel } from "./TranslationPanel";

vi.mock("./MarkdownTranslation", () => ({
  MarkdownTranslation: ({ text }: { text: string }) => (
    <div data-testid="markdown">{text}</div>
  ),
}));

describe("TranslationPanel", () => {
  it("renders streaming text as plain text with a readable label", () => {
    render(<TranslationPanel text="**unfinished" mode="streaming" />);

    expect(screen.getByText("生成中")).toBeInTheDocument();
    expect(screen.getByText("**unfinished")).toHaveClass("whitespace-pre-wrap");
    expect(screen.queryByTestId("markdown")).not.toBeInTheDocument();
  });

  it("labels partial text and keeps it out of the Markdown renderer", () => {
    render(<TranslationPanel text="partial" mode="partial" />);

    expect(screen.getByText("未完成译文")).toBeInTheDocument();
    expect(screen.queryByTestId("markdown")).not.toBeInTheDocument();
  });

  it("uses the Markdown renderer only for complete text", async () => {
    render(<TranslationPanel text="complete" mode="complete" />);

    expect(await screen.findByTestId("markdown")).toHaveTextContent("complete");
    expect(screen.queryByText("生成中")).not.toBeInTheDocument();
  });
});
