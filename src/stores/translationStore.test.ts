import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createTranslationDeltaBuffer,
  useTranslationStore,
} from "./translationStore";

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
    });

    useTranslationStore.getState().succeedRequest(requestId, "译文");
    expect(useTranslationStore.getState()).toMatchObject({
      translatedText: "译文",
      status: "success",
      isPartial: false,
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
