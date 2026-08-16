import { describe, expect, it } from "vitest";
import { formatCommandError, toFriendlyError } from "./tauriCommands";

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
