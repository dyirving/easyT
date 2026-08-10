import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTranslationStore } from "@/stores/translationStore";
import { useSettingsStore } from "@/stores/settingsStore";
import { TranslationPage } from "./TranslationPage";

vi.mock("@/components/translation/MarkdownTranslation", () => ({
  MarkdownTranslation: ({ text }: { text: string }) => (
    <div data-testid="markdown">{text}</div>
  ),
}));

vi.mock("@/services/translationRunner", () => ({
  runTranslationRequest: vi.fn(),
}));

import { runTranslationRequest } from "@/services/translationRunner";
const mockedRunTranslationRequest = vi.mocked(runTranslationRequest);

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
    useTranslationStore
      .getState()
      .succeedRequest(requestId, {
        translatedText: "complete",
        fromCache: false,
      });

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

describe("TranslationPage refresh intent", () => {
  beforeEach(() => {
    useTranslationStore.getState().reset();
    useSettingsStore.getState().resetToDefault();
    mockedRunTranslationRequest.mockReset().mockResolvedValue(undefined);
  });

  it("header retry re-translates the same text with forceRefresh", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .succeedRequest(requestId, {
        translatedText: "译文",
        fromCache: true,
      });

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);
    const refreshButton = screen.getByRole("button", { name: "重新翻译" });
    refreshButton.click();

    expect(mockedRunTranslationRequest).toHaveBeenCalledOnce();
    const [id, text, , forceRefresh] =
      mockedRunTranslationRequest.mock.calls[0];
    expect(id).not.toBe(requestId);
    expect(text).toBe("source");
    expect(forceRefresh).toBe(true);
  });

  it("error retry re-requests the same text with forceRefresh", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .failRequest(requestId, "请求超时", "ApiTimeout", "source");

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    const retryButton = screen.getByRole("button", { name: "重试" });
    retryButton.click();

    expect(mockedRunTranslationRequest).toHaveBeenCalledOnce();
    expect(mockedRunTranslationRequest.mock.calls[0][1]).toBe("source");
    expect(mockedRunTranslationRequest.mock.calls[0][3]).toBe(true);
  });

  it("manual translate sends an ordinary (non-refresh) request", async () => {
    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    const translateButton = screen.getByRole("button", { name: "翻译" });
    translateButton.click();

    expect(mockedRunTranslationRequest).toHaveBeenCalledOnce();
    expect(mockedRunTranslationRequest.mock.calls[0][3]).toBe(false);
  });

  it("refreshing keeps the cached text visible with a non-blocking indicator", async () => {
    const firstId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .succeedRequest(firstId, {
        translatedText: "缓存译文",
        fromCache: true,
      });
    const { unmount } = render(
      <TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />,
    );
    expect(screen.queryByText("正在重新翻译")).not.toBeInTheDocument();
    unmount();

    useTranslationStore.getState().startRequest("source", true);
    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText("正在重新翻译")).toBeInTheDocument();
    expect(
      screen.getByText(/此译文来自本机缓存，点击“重新翻译”/),
    ).toBeInTheDocument();
    expect(await screen.findByTestId("markdown")).toHaveTextContent(
      "缓存译文",
    );
  });

  it("failed refresh keeps the cached text and shows the refresh error", async () => {
    const firstId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .succeedRequest(firstId, {
        translatedText: "缓存译文",
        fromCache: true,
      });
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);
    useTranslationStore
      .getState()
      .failRefreshRequest(refreshId, "网络请求失败", "BackendNetwork");

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(
      screen.getByText(/重新翻译失败，当前仍显示此前的本机缓存译文/),
    ).toBeInTheDocument();
    expect(await screen.findByTestId("markdown")).toHaveTextContent(
      "缓存译文",
    );
  });

  it("cached success shows the cache notice between original and translation", async () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .succeedRequest(requestId, {
        translatedText: "缓存译文",
        fromCache: true,
      });

    render(<TranslationPage onOpenSettings={vi.fn()} onClose={vi.fn()} />);

    expect(
      screen.getByText(/此译文来自本机缓存，点击“重新翻译”/),
    ).toBeInTheDocument();
    expect(await screen.findByTestId("markdown")).toHaveTextContent(
      "缓存译文",
    );
  });
});
