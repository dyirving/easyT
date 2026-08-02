import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTranslationStore } from "@/stores/translationStore";
import { TranslationPage } from "./TranslationPage";

vi.mock("@/components/MarkdownTranslation", () => ({
  MarkdownTranslation: ({ text }: { text: string }) => (
    <div data-testid="markdown">{text}</div>
  ),
}));

describe("TranslationPage copy state", () => {
  beforeEach(() => useTranslationStore.getState().reset());

  it("shows partial text and disables copying after a streaming failure", () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .appendTranslationDelta(requestId, "partial");
    useTranslationStore
      .getState()
      .failRequest(requestId, "timeout", "ApiTimeout", undefined, true);

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText("未完成译文")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "复制译文" })).toBeDisabled();
  });

  it("enables copying only after complete success", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore.getState().succeedRequest(requestId, "complete");

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByRole("button", { name: "复制译文" })).toBeEnabled();
    expect(await screen.findByTestId("markdown")).toHaveTextContent("complete");
  });
});
