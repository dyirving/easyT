import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createTranslationDeltaBuffer,
  useTranslationStore,
} from "./translationStore";

const cachedResult = { translatedText: "缓存译文", fromCache: true, totalElapsedMs: 8 };
const freshResult = { translatedText: "新译文", fromCache: false, totalElapsedMs: 1200 };

describe("translationStore streaming state", () => {
  beforeEach(() => useTranslationStore.getState().reset());
  afterEach(() => vi.useRealTimers());

  it("appends active deltas and completes with the final text", () => {
    const requestId = useTranslationStore.getState().startRequest("source");

    expect(
      useTranslationStore.getState().appendTranslationDelta(requestId, "译"),
    ).toBe(true);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "译",
      status: "streaming",
      isPartial: false,
      fromCache: false,
    });

    useTranslationStore.getState().succeedRequest(requestId, freshResult);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "新译文",
      status: "success",
      isPartial: false,
      fromCache: false,
    });
  });

  it("preserves partial text on ordinary failure but not cancellation", () => {
    const partialRequest = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .appendTranslationDelta(partialRequest, "未完成");
    useTranslationStore
      .getState()
      .failRequest(partialRequest, "超时", "ApiTimeout", undefined, true);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "未完成",
      status: "error",
      isPartial: true,
    });

    const cancelledRequest = useTranslationStore.getState().startRequest("next");
    useTranslationStore
      .getState()
      .appendTranslationDelta(cancelledRequest, "discard");
    useTranslationStore
      .getState()
      .failRequest(cancelledRequest, "取消", "BackendCancelled");
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "",
      status: "error",
      isPartial: false,
    });
  });

  it("rejects stale deltas and drops a stale buffer after supersession", () => {
    vi.useFakeTimers();
    const oldRequest = useTranslationStore.getState().startRequest("old");
    const buffer = createTranslationDeltaBuffer(oldRequest);
    buffer.append("stale");

    const newRequest = useTranslationStore.getState().startRequest("new");
    expect(
      useTranslationStore.getState().appendTranslationDelta(oldRequest, "late"),
    ).toBe(false);
    vi.advanceTimersByTime(50);

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: newRequest,
      translatedText: "",
      status: "translating",
    });
    buffer.dispose();
  });

  it("batches deltas into at most one update per 50ms window", () => {
    vi.useFakeTimers();
    const requestId = useTranslationStore.getState().startRequest("source");
    const buffer = createTranslationDeltaBuffer(requestId);
    const append = vi.spyOn(
      useTranslationStore.getState(),
      "appendTranslationDelta",
    );

    buffer.append("a");
    buffer.append("b");
    buffer.append("c");
    expect(useTranslationStore.getState().translatedText).toBe("");
    vi.advanceTimersByTime(50);

    expect(append).toHaveBeenCalledOnce();
    expect(useTranslationStore.getState().translatedText).toBe("abc");
    buffer.dispose();
  });
});

describe("translationStore request progress", () => {
  beforeEach(() => useTranslationStore.getState().reset());

  it("accepts only current, increasing, non-backward phase events", () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    const checking = {
      type: "phaseChanged" as const,
      requestId,
      sequence: 1,
      phase: "checkingCache" as const,
      totalElapsedMs: 12,
    };
    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, checking),
    ).toBe(true);
    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, checking),
    ).toBe(false);

    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, {
        ...checking,
        sequence: 2,
        phase: "connectingBackend",
        totalElapsedMs: 120,
        backend: { mode: "officialApi", provider: "deepseek" },
      }),
    ).toBe(true);
    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, {
        ...checking,
        sequence: 3,
        phase: "preparingRequest",
        totalElapsedMs: 130,
      }),
    ).toBe(false);
    expect(useTranslationStore.getState()).toMatchObject({
      progressPhase: "connectingBackend",
      progressSequence: 2,
      totalElapsedMs: 120,
    });
  });

  it("resets a same-phase retry and keeps only the Rust terminal duration", () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    const event = {
      type: "phaseChanged" as const,
      requestId,
      sequence: 1,
      phase: "connectingBackend" as const,
      totalElapsedMs: 100,
      backend: { mode: "webGateway" as const, provider: "qwen" },
    };
    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, event),
    ).toBe(true);
    expect(
      useTranslationStore.getState().applyProgressPhase(requestId, {
        ...event,
        sequence: 2,
        totalElapsedMs: 800,
      }),
    ).toBe(true);

    useTranslationStore.getState().succeedRequest(requestId, {
      translatedText: "译文",
      fromCache: false,
      totalElapsedMs: 14600,
    });
    expect(useTranslationStore.getState()).toMatchObject({
      progressPhase: null,
      progressSequence: null,
      totalElapsedMs: 14600,
      status: "success",
    });
  });
});

describe("translationStore failCapture", () => {
  beforeEach(() => useTranslationStore.getState().reset());

  it("atomically replaces an active request with a no-text error state", () => {
    const requestId = useTranslationStore.getState().startRequest("source");
    useTranslationStore
      .getState()
      .appendTranslationDelta(requestId, "partial");
    useTranslationStore.getState().setPinned(true);

    useTranslationStore.getState().failCapture("剪贴板被占用", "ClipboardError");

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: null,
      originalText: "",
      translatedText: "",
      status: "error",
      errorMessage: "剪贴板被占用",
      errorKind: "ClipboardError",
      isPartial: false,
      pinned: true,
    });
    expect(
      useTranslationStore
        .getState()
        .succeedRequest(requestId, { translatedText: "迟到", fromCache: false, totalElapsedMs: 1 }),
    ).toBe(false);
    expect(
      useTranslationStore.getState().appendTranslationDelta(requestId, "迟到"),
    ).toBe(false);
  });

  it("defaults to a null error kind when omitted", () => {
    useTranslationStore.getState().failCapture("捕获失败");

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: null,
      status: "error",
      errorMessage: "捕获失败",
      errorKind: null,
    });
  });
});

describe("translationStore refresh state machine", () => {
  beforeEach(() => useTranslationStore.getState().reset());

  function cachedSuccess(originalText: string) {
    const requestId = useTranslationStore.getState().startRequest(originalText);
    useTranslationStore.getState().succeedRequest(requestId, cachedResult);
  }

  it("refresh keeps a same-text cached result visible while refreshing", () => {
    cachedSuccess("source");
    useTranslationStore.getState().setPinned(true);

    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: refreshId,
      originalText: "source",
      translatedText: "缓存译文",
      status: "refreshing",
      fromCache: true,
      refreshErrorMessage: null,
      pinned: true,
    });
  });

  it("refresh with a non-cached current result behaves like a normal start", () => {
    const plainId = useTranslationStore.getState().startRequest("source");
    useTranslationStore.getState().succeedRequest(plainId, freshResult);

    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: refreshId,
      translatedText: "",
      status: "translating",
      fromCache: false,
    });
  });

  it("refresh with a different text behaves like a normal start", () => {
    cachedSuccess("source");

    const refreshId = useTranslationStore
      .getState()
      .startRequest("other", true);

    expect(useTranslationStore.getState()).toMatchObject({
      requestId: refreshId,
      translatedText: "",
      status: "translating",
      fromCache: false,
    });
  });

  it("ordinary start clears cached text, fromCache and refresh error", () => {
    cachedSuccess("source");
    const failedRefresh = useTranslationStore
      .getState()
      .startRequest("source", true);
    useTranslationStore
      .getState()
      .failRefreshRequest(failedRefresh, "网络请求失败", "BackendNetwork");

    const requestId = useTranslationStore.getState().startRequest("source");

    expect(useTranslationStore.getState()).toMatchObject({
      requestId,
      translatedText: "",
      status: "translating",
      fromCache: false,
      refreshErrorMessage: null,
    });
  });

  it("successful refresh replaces the text and adopts the result cache flag", () => {
    cachedSuccess("source");
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    expect(
      useTranslationStore.getState().succeedRequest(refreshId, freshResult),
    ).toBe(true);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "新译文",
      status: "success",
      fromCache: false,
      refreshErrorMessage: null,
    });
  });

  it("failed refresh keeps the cached text and reports a refresh error", () => {
    cachedSuccess("source");
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    expect(
      useTranslationStore
        .getState()
        .failRefreshRequest(refreshId, "网络请求失败", "BackendNetwork"),
    ).toBe(true);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "缓存译文",
      status: "success",
      fromCache: true,
      refreshErrorMessage: "网络请求失败",
      errorKind: null,
    });
  });

  it("failRefreshRequest rejects stale or non-refreshing states", () => {
    const plainId = useTranslationStore.getState().startRequest("source");
    expect(
      useTranslationStore.getState().failRefreshRequest(plainId, "失败"),
    ).toBe(false);

    cachedSuccess("source");
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);
    useTranslationStore
      .getState()
      .succeedRequest(refreshId, freshResult);
    expect(
      useTranslationStore.getState().failRefreshRequest(refreshId, "迟到"),
    ).toBe(false);
  });

  it("stale refreshes cannot overwrite a newer request", () => {
    cachedSuccess("source");
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    useTranslationStore.getState().startRequest("newer");
    expect(
      useTranslationStore.getState().succeedRequest(refreshId, freshResult),
    ).toBe(false);
    expect(
      useTranslationStore.getState().failRefreshRequest(refreshId, "迟到"),
    ).toBe(false);
  });

  it("deltas are not accepted while refreshing preserves old text", () => {
    cachedSuccess("source");
    const refreshId = useTranslationStore
      .getState()
      .startRequest("source", true);

    expect(
      useTranslationStore.getState().appendTranslationDelta(refreshId, "增量"),
    ).toBe(false);
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "缓存译文",
      status: "refreshing",
    });
  });

  it("clearCacheSourceNotice keeps the text but drops the cache flag", () => {
    cachedSuccess("source");
    useTranslationStore.getState().clearCacheSourceNotice();

    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "缓存译文",
      status: "success",
      fromCache: false,
    });
  });
});
