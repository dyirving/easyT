import type { TermEntry, TermbaseSnapshot } from "@/types";

/** 术语表测试支撑：settings 目录下两个测试文件共用（entry/snapshot 构造）。 */
export function termEntry(
  id: string,
  source: string,
  target: string,
): TermEntry {
  return {
    id,
    sourceTerm: source,
    targetLanguage: "简体中文",
    targetTerm: target,
    enabled: true,
    caseSensitive: false,
    createdAtUtcMs: 1,
    updatedAtUtcMs: 1,
  };
}

export function termbaseSnapshot(
  entries: TermEntry[],
  enabled = true,
): TermbaseSnapshot {
  return { enabled, entries, maximumEntries: 200 };
}
