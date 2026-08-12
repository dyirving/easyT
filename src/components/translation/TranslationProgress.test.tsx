import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  formatActiveDuration,
  formatTerminalDuration,
  TranslationProgress,
} from "./TranslationProgress";

describe("TranslationProgress", () => {
  it("shows a real phase, backend hint, and separate timing", () => {
    render(
      <TranslationProgress
        kind="active"
        compact={false}
        snapshot={{
          phase: "connectingBackend",
          sequence: 3,
          backend: { mode: "officialApi", provider: "deepseek" },
          phaseStartedTotalElapsedMs: 1000,
          syncedTotalElapsedMs: 3000,
          syncedAtMonotonicMs: performance.now(),
          requestStartedAtMonotonicMs: performance.now() - 3000,
        }}
      />,
    );

    expect(screen.getByText("正在连接翻译服务")).toBeInTheDocument();
    expect(screen.getByText("Official API · DeepSeek")).toBeInTheDocument();
    expect(screen.getByText("本阶段 2 秒 · 总计 3 秒")).toBeInTheDocument();
  });

  it("formats active and terminal boundaries from the agreed literals", () => {
    expect(formatActiveDuration(999)).toBe("不足 1 秒");
    expect(formatActiveDuration(1000)).toBe("1 秒");
    expect(formatActiveDuration(1999)).toBe("1 秒");
    expect(formatTerminalDuration(99)).toBe("不足 0.1 秒");
    expect(formatTerminalDuration(100)).toBe("0.1 秒");
    expect(formatTerminalDuration(9840)).toBe("9.8 秒");
    expect(formatTerminalDuration(9950)).toBe("10 秒");
    expect(formatTerminalDuration(14600)).toBe("15 秒");
  });

  it("uses the neutral fallback before the first real phase", () => {
    const startedAt = performance.now();
    render(
      <TranslationProgress
        kind="active"
        compact={false}
        snapshot={{
          phase: null,
          sequence: null,
          backend: null,
          phaseStartedTotalElapsedMs: null,
          syncedTotalElapsedMs: null,
          syncedAtMonotonicMs: null,
          requestStartedAtMonotonicMs: startedAt,
        }}
      />,
    );

    expect(screen.getByText("正在处理翻译请求")).toBeInTheDocument();
    expect(screen.getByText("总计不足 1 秒")).toBeInTheDocument();
  });
});
