import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  createTermbaseEntry,
  deleteTermbaseEntry,
  getTermbase,
  setTermbaseEnabled,
  setTermbaseEntryEnabled,
  updateTermbaseEntry,
} from "@/services/tauriCommands";
import {
  TERMBASE_PAGE_SIZE,
  useTermbaseController,
} from "./useTermbaseController";
import { termEntry, termbaseSnapshot } from "./termbaseTestUtils";

vi.mock("@/services/tauriCommands", () => ({
  createTermbaseEntry: vi.fn(),
  deleteTermbaseEntry: vi.fn(),
  getTermbase: vi.fn(),
  setTermbaseEnabled: vi.fn(),
  setTermbaseEntryEnabled: vi.fn(),
  toCommandError: (error: unknown) => ({
    kind: "ConfigInvalid",
    message: error instanceof Error ? error.message : "操作失败",
  }),
  updateTermbaseEntry: vi.fn(),
}));

describe("useTermbaseController", () => {
  beforeEach(() => vi.clearAllMocks());

  it("loads the authoritative snapshot on mount", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));
    expect(result.current.snapshot?.entries).toHaveLength(1);
    expect(result.current.totalCount).toBe(1);
  });

  it("surfaces load failures in the banner", async () => {
    vi.mocked(getTermbase).mockRejectedValue(new Error("读取失败"));
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("error"));
    expect(result.current.bannerMessage).toContain("读取术语表失败");
  });

  it("filters by case-insensitive containment on source and target terms", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([
        termEntry("1", "neural network", "神经网络"),
        termEntry("2", "function", "函数"),
        termEntry("3", "China", "中国"),
      ]),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    act(() => result.current.setQuery("NEURAL"));
    expect(result.current.visibleEntries.map((e) => e.id)).toEqual(["1"]);

    act(() => result.current.setQuery("函数"));
    expect(result.current.visibleEntries.map((e) => e.id)).toEqual(["2"]);

    act(() => result.current.setQuery(""));
    expect(result.current.visibleEntries).toHaveLength(3);
  });

  it("paginates at 20 entries and resets to page one on query change", async () => {
    const entries = Array.from({ length: 25 }, (_, i) =>
      termEntry(String(i), `term-${i}`, `译-${i}`),
    );
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot(entries));
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    expect(result.current.pageCount).toBe(2);
    expect(result.current.visibleEntries).toHaveLength(TERMBASE_PAGE_SIZE);

    act(() => result.current.setPage(2));
    expect(result.current.visibleEntries).toHaveLength(5);

    act(() => result.current.setQuery("term-0"));
    expect(result.current.page).toBe(1);
  });

  it("replaces the snapshot after a successful mutation and resets page", async () => {
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot([]));
    vi.mocked(setTermbaseEnabled).mockResolvedValue(termbaseSnapshot([], false));
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    act(() => result.current.setPage(1));
    await act(async () => {
      await result.current.toggleEnabled();
    });
    expect(setTermbaseEnabled).toHaveBeenCalledWith(false);
    expect(result.current.snapshot?.enabled).toBe(false);
  });

  it("keeps the snapshot and shows a banner when a mutation fails", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    vi.mocked(setTermbaseEntryEnabled).mockRejectedValue(
      new Error("术语“function”与现有条目冲突"),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    await act(async () => {
      await result.current.toggleEntry(result.current.snapshot!.entries[0]);
    });
    expect(result.current.bannerMessage).toContain(
      "术语“function”与现有条目冲突",
    );
    expect(result.current.snapshot?.entries).toHaveLength(1);
  });

  it("saves a create draft through createTermbaseEntry", async () => {
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot([]));
    vi.mocked(createTermbaseEntry).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    act(() => result.current.beginCreate());
    act(() =>
      result.current.updateDraft({
        sourceTerm: "  function  ",
        targetLanguage: "简体中文",
        targetTerm: "  函数  ",
      }),
    );
    await act(async () => {
      await result.current.saveDraft();
    });

    expect(createTermbaseEntry).toHaveBeenCalledWith({
      sourceTerm: "function",
      targetLanguage: "简体中文",
      targetTerm: "函数",
      caseSensitive: false,
    });
    expect(result.current.editing).toBeNull();
    expect(result.current.snapshot?.entries).toHaveLength(1);
  });

  it("saves an edit draft through updateTermbaseEntry", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    vi.mocked(updateTermbaseEntry).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "功能")]),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    act(() => result.current.beginEdit(result.current.snapshot!.entries[0]));
    expect(result.current.draft?.sourceTerm).toBe("function");
    act(() => result.current.updateDraft({ targetTerm: "功能" }));
    await act(async () => {
      await result.current.saveDraft();
    });

    expect(updateTermbaseEntry).toHaveBeenCalledWith("1", {
      sourceTerm: "function",
      targetLanguage: "简体中文",
      targetTerm: "功能",
      caseSensitive: false,
    });
  });

  it("confirms deletion only after requestDelete", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    vi.mocked(deleteTermbaseEntry).mockResolvedValue(termbaseSnapshot([]));
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    expect(deleteTermbaseEntry).not.toHaveBeenCalled();
    act(() => result.current.requestDelete(result.current.snapshot!.entries[0]));
    expect(result.current.deleting).toBe("1");

    await act(async () => {
      await result.current.confirmDelete();
    });
    expect(deleteTermbaseEntry).toHaveBeenCalledWith("1");
    expect(result.current.deleting).toBeNull();
    expect(result.current.snapshot?.entries).toHaveLength(0);
  });

  it("closeView resets query, page, drafts and delete intent", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    const { result } = renderHook(() => useTermbaseController());
    await waitFor(() => expect(result.current.phase).toBe("ready"));

    act(() => {
      result.current.setQuery("fun");
      result.current.beginCreate();
      result.current.requestDelete(result.current.snapshot!.entries[0]);
    });
    act(() => result.current.closeView());
    expect(result.current.query).toBe("");
    expect(result.current.page).toBe(1);
    expect(result.current.editing).toBeNull();
    expect(result.current.deleting).toBeNull();
  });
});