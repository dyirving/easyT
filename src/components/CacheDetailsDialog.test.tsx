import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { CacheDetailsDialog } from "./CacheDetailsDialog";
import { getTranslationCacheStats } from "@/services/tauriCommands";
import type { CacheStats } from "@/types";

vi.mock("@/services/tauriCommands", () => ({
  getTranslationCacheStats: vi.fn(),
  toCommandError: (error: unknown) => ({
    kind: "CacheOperationFailed",
    message: error instanceof Error ? error.message : "读取失败",
  }),
}));

const readyStats: CacheStats = {
  state: "ready",
  entryCount: 12,
  diskBytes: 1_572_864,
  maxDiskBytes: 268_435_456,
  hitRate: 2 / 3,
  cachePath: "D:\\easyT_Data\\cache\\translation_cache.sqlite3",
};

describe("CacheDetailsDialog", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows loading then ready statistics and local-storage disclosure", async () => {
    let resolveStats: (stats: CacheStats) => void = () => {};
    vi.mocked(getTranslationCacheStats).mockReturnValue(
      new Promise((resolve) => {
        resolveStats = resolve;
      }),
    );

    render(<CacheDetailsDialog open onClose={vi.fn()} />);
    expect(screen.getByRole("dialog", { name: "翻译缓存详情" })).toHaveAttribute(
      "aria-modal",
      "true",
    );
    expect(screen.getByText("正在读取缓存详情…")).toBeInTheDocument();

    resolveStats(readyStats);
    expect(await screen.findByText("12 条")).toBeInTheDocument();
    expect(screen.getByText("1.5 MiB / 256.0 MiB")).toBeInTheDocument();
    expect(screen.getByText("66.7%")).toBeInTheDocument();
    expect(screen.getByText(readyStats.cachePath)).toBeInTheDocument();
    expect(screen.getByText(/译文以明文保存在本机/)).toBeInTheDocument();
  });

  it("shows degraded state and an em dash for an empty hit-rate denominator", async () => {
    vi.mocked(getTranslationCacheStats).mockResolvedValue({
      ...readyStats,
      state: "degraded",
      entryCount: 0,
      hitRate: null,
    });
    render(<CacheDetailsDialog open onClose={vi.fn()} />);

    expect(await screen.findByText("持久化缓存不可用")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
  });

  it("shows a safe query failure state", async () => {
    vi.mocked(getTranslationCacheStats).mockRejectedValue(
      new Error("无法读取缓存详情"),
    );
    render(<CacheDetailsDialog open onClose={vi.fn()} />);
    expect(await screen.findByText(/读取缓存详情失败/)).toBeInTheDocument();
  });

  it("closes with Escape and restores focus to the trigger", async () => {
    vi.mocked(getTranslationCacheStats).mockResolvedValue(readyStats);

    function Harness() {
      const [open, setOpen] = useState(false);
      return (
        <>
          <button onClick={() => setOpen(true)}>查看缓存详情</button>
          <CacheDetailsDialog open={open} onClose={() => setOpen(false)} />
        </>
      );
    }

    render(<Harness />);
    const trigger = screen.getByRole("button", { name: "查看缓存详情" });
    trigger.focus();
    fireEvent.click(trigger);
    await screen.findByRole("dialog", { name: "翻译缓存详情" });
    expect(screen.getByRole("button", { name: "关闭缓存详情" })).toHaveFocus();

    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });
});
