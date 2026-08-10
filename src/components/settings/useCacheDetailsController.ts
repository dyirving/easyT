import { useEffect, useState } from "react";
import { clearTranslationCache, getTranslationCacheStats, toCommandError } from "@/services/tauriCommands";
import type { CacheStats, PersistentCacheState } from "@/types";

type DetailsState = { phase: "loading" } | { phase: "ready"; stats: CacheStats } | { phase: "error"; message: string };

export function useCacheDetailsController(open: boolean, onCacheCleared: () => void) {
  const [details, setDetails] = useState<DetailsState>({ phase: "loading" });
  const [clearing, setClearing] = useState(false);
  const [confirming, setConfirming] = useState<PersistentCacheState | null>(null);
  useEffect(() => { if (!open) return; let cancelled = false; setDetails({ phase: "loading" }); getTranslationCacheStats().then((stats) => !cancelled && setDetails({ phase: "ready", stats })).catch((error) => !cancelled && setDetails({ phase: "error", message: `读取缓存详情失败：${toCommandError(error).message}` })); return () => { cancelled = true; }; }, [open]);
  const requestClear = () => { if (!clearing) setConfirming(details.phase === "ready" ? details.stats.state : "ready"); };
  const confirmClear = async () => { if (clearing) return; setConfirming(null); setClearing(true); try { const stats = await clearTranslationCache(); setDetails({ phase: "ready", stats }); onCacheCleared(); } catch (error) { setDetails({ phase: "error", message: `清除翻译缓存失败：${toCommandError(error).message}` }); } finally { setClearing(false); } };
  return { details, clearing, confirming, setConfirming, requestClear, confirmClear };
}
