import { useEffect, useMemo, useState } from "react";
import {
  createTermbaseEntry,
  deleteTermbaseEntry,
  getTermbase,
  setTermbaseEnabled,
  setTermbaseEntryEnabled,
  toCommandError,
  updateTermbaseEntry,
} from "@/services/tauriCommands";
import type { TermEntry, TermEntryInput, TermbaseSnapshot } from "@/types";

/** 分页大小：每页 20 条（SDD §6.4） */
export const TERMBASE_PAGE_SIZE = 20;

export interface TermDraft {
  sourceTerm: string;
  targetLanguage: string;
  targetTerm: string;
  caseSensitive: boolean;
}

export type TermbasePhase = "loading" | "ready" | "error";

export const EMPTY_DRAFT: TermDraft = {
  sourceTerm: "",
  targetLanguage: "简体中文",
  targetTerm: "",
  caseSensitive: false,
};

/**
 * 术语表控制器：页面挂载时取一次权威快照（显示条目数），
 * 所有 mutation 成功后用 Rust 返回的快照整体替换本地状态。
 */
export function useTermbaseController() {
  const [phase, setPhase] = useState<TermbasePhase>("loading");
  const [snapshot, setSnapshot] = useState<TermbaseSnapshot | null>(null);
  const [bannerMessage, setBannerMessage] = useState("");
  const [query, setQuery] = useState("");
  const [page, setPage] = useState(1);
  const [editing, setEditing] = useState<null | "create" | string>(null);
  const [draft, setDraft] = useState<TermDraft | null>(null);
  const [deleting, setDeleting] = useState<null | string>(null);
  const [pending, setPending] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getTermbase()
      .then((next) => {
        if (cancelled) return;
        setSnapshot(next);
        setPhase("ready");
      })
      .catch((error) => {
        if (cancelled) return;
        setBannerMessage(`读取术语表失败：${toCommandError(error).message}`);
        setPhase("error");
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** 客户端搜索：源术语与指定译法的忽略大小写包含匹配 */
  const filtered = useMemo(() => {
    if (!snapshot) return [];
    const q = query.trim().toLowerCase();
    if (!q) return snapshot.entries;
    return snapshot.entries.filter(
      (entry) =>
        entry.sourceTerm.toLowerCase().includes(q) ||
        entry.targetTerm.toLowerCase().includes(q),
    );
  }, [snapshot, query]);

  const pageCount = Math.max(1, Math.ceil(filtered.length / TERMBASE_PAGE_SIZE));
  const visibleEntries = filtered.slice(
    (page - 1) * TERMBASE_PAGE_SIZE,
    page * TERMBASE_PAGE_SIZE,
  );

  const runMutation = async (
    action: () => Promise<TermbaseSnapshot>,
  ): Promise<TermbaseSnapshot | null> => {
    if (pending) return null;
    setPending(true);
    try {
      const next = await action();
      setSnapshot(next);
      setPhase("ready");
      setBannerMessage("");
      setPage(1);
      return next;
    } catch (error) {
      setBannerMessage(`操作失败：${toCommandError(error).message}`);
      return null;
    } finally {
      setPending(false);
    }
  };

  const toggleEnabled = () => {
    if (!snapshot || pending) return;
    void runMutation(() => setTermbaseEnabled(!snapshot.enabled));
  };

  const toggleEntry = (entry: TermEntry) => {
    if (pending) return;
    void runMutation(() => setTermbaseEntryEnabled(entry.id, !entry.enabled));
  };

  const beginCreate = () => {
    setEditing("create");
    setDraft(EMPTY_DRAFT);
  };

  const beginEdit = (entry: TermEntry) => {
    setEditing(entry.id);
    setDraft({
      sourceTerm: entry.sourceTerm,
      targetLanguage: entry.targetLanguage,
      targetTerm: entry.targetTerm,
      caseSensitive: entry.caseSensitive,
    });
  };

  const updateDraft = (patch: Partial<TermDraft>) => {
    setDraft((current) => (current ? { ...current, ...patch } : current));
  };

  const saveDraft = async () => {
    if (!draft || pending) return;
    const input: TermEntryInput = {
      sourceTerm: draft.sourceTerm.trim(),
      targetLanguage: draft.targetLanguage,
      targetTerm: draft.targetTerm.trim(),
      caseSensitive: draft.caseSensitive,
    };
    const next = await runMutation(() =>
      editing === "create"
        ? createTermbaseEntry(input)
        : updateTermbaseEntry(editing as string, input),
    );
    if (next) {
      setEditing(null);
      setDraft(null);
    }
  };

  const requestDelete = (entry: TermEntry) => setDeleting(entry.id);

  const confirmDelete = async () => {
    if (!deleting || pending) return;
    const id = deleting;
    const next = await runMutation(() => deleteTermbaseEntry(id));
    if (next) setDeleting(null);
  };

  const cancelDelete = () => setDeleting(null);

  const cancelEdit = () => {
    setEditing(null);
    setDraft(null);
  };

  /** 关闭对话框时复位视图状态（草稿、搜索、分页、删除意图） */
  const closeView = () => {
    cancelEdit();
    setDeleting(null);
    setQuery("");
    setPage(1);
  };

  return {
    phase,
    snapshot,
    bannerMessage,
    query,
    setQuery: (value: string) => {
      setQuery(value);
      setPage(1);
    },
    page,
    setPage,
    pageCount,
    visibleEntries,
    totalCount: snapshot?.entries.length ?? 0,
    editing,
    draft,
    updateDraft,
    beginCreate,
    beginEdit,
    saveDraft,
    cancelEdit,
    deleting,
    requestDelete,
    confirmDelete,
    cancelDelete,
    toggleEnabled,
    toggleEntry,
    pending,
    closeView,
  };
}
