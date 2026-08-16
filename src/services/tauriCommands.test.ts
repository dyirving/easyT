import { describe, expect, it } from "vitest";
import {
  formatCommandError,
  TERMBASE_CONTEXT_LENGTH_MESSAGE,
  TERMBASE_CONTEXT_SUGGESTION,
  toFriendlyError,
} from "./tauriCommands";

describe("Qwen command errors", () => {
  it("keeps a structured Qwen code through friendly error formatting", () => {
    const error = {
      kind: "BackendNetwork" as const,
      message: "Qwen 请求过于频繁",
      code: "QW-UPSTREAM-429",
    };

    expect(formatCommandError(error)).toBe("Qwen 请求过于频繁 [QW-UPSTREAM-429]");
    expect(toFriendlyError(error)).toMatchObject({
      friendlyMessage: "Qwen 请求过于频繁",
      code: "QW-UPSTREAM-429",
    });
  });

  it("does not append a code for existing Official API errors", () => {
    expect(formatCommandError({ message: "请求超时" })).toBe("请求超时");
  });
});

describe("FR-010 termbase context hints", () => {
  it("recognized context-length errors map to the dedicated message and suggestion", () => {
    const friendly = toFriendlyError({
      kind: "BackendInvalidResponse",
      message: TERMBASE_CONTEXT_LENGTH_MESSAGE,
    });
    expect(friendly.friendlyMessage).toBe(TERMBASE_CONTEXT_LENGTH_MESSAGE);
    expect(friendly.hint).toBe(TERMBASE_CONTEXT_SUGGESTION);
    expect(friendly.retryable).toBe(true);
  });

  it("generic failures carrying the suggestion append it to the hint", () => {
    const friendly = toFriendlyError({
      kind: "BackendNetwork",
      message: `网络请求失败。${TERMBASE_CONTEXT_SUGGESTION}`,
    });
    expect(friendly.friendlyMessage).toBe("网络请求失败");
    expect(friendly.hint).toContain(TERMBASE_CONTEXT_SUGGESTION);
  });

  it("keeps existing hint text when appending the suggestion", () => {
    const friendly = toFriendlyError({
      kind: "BackendInvalidResponse",
      message: `响应格式无效。${TERMBASE_CONTEXT_SUGGESTION}`,
    });
    expect(friendly.hint).toContain("Qwen 返回内容无法解析");
    expect(friendly.hint).toContain(TERMBASE_CONTEXT_SUGGESTION);
  });

  it("leaves unrelated errors untouched", () => {
    const friendly = toFriendlyError({
      kind: "BackendNetwork",
      message: "网络请求失败",
    });
    expect(friendly.friendlyMessage).toBe("网络请求失败");
    expect(friendly.hint).not.toContain("术语表");
  });
});
