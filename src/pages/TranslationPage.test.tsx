import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTranslationStore } from "@/stores/translationStore";
import { useSettingsStore } from "@/stores/settingsStore";
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

describe("TranslationPage shortcut behavior", () => {
  beforeEach(() => {
    useTranslationStore.getState().reset();
    useSettingsStore.getState().resetToDefault();
  });

  it("T-012 shows the configured shortcut and both behaviors in idle", () => {
    useSettingsStore.getState().setConfig({ shortcut: "Alt+Shift+D" });

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText("Alt+Shift+D")).toBeInTheDocument();
    expect(screen.getByText(/有选区时按/)).toBeInTheDocument();
    expect(screen.getByText(/无选区时显示翻译窗口/)).toBeInTheDocument();
    expect(
      screen.getByPlaceholderText(
        "例如：Large language models are trained on massive text corpora.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("Ctrl+T")).not.toBeInTheDocument();
  });

  it("T-013 hides retry for capture failures without original text", () => {
    useTranslationStore.getState().failCapture("剪贴板被占用", "ClipboardError");

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText("剪贴板被占用")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "重试" }),
    ).not.toBeInTheDocument();
  });

  it("T-013 keeps retry for retryable translation failures with original text", () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .failRequest(requestId, "请求超时", "ApiTimeout", "source");

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText("请求超时")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "重试" })).toBeInTheDocument();
  });
});
