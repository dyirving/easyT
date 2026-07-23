import { useEffect, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TranslationPage } from "@/pages/TranslationPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import {
  captureSelectedText,
  getConfig,
  positionWindowNearMouse,
  toCommandError,
  translateText,
} from "@/services/tauriCommands";

type Route = "translation" | "settings";

// 模块级 in-flight 标志：防止用户连按快捷键触发并发翻译
// 重入时直接丢弃后续请求，让正在进行的请求完成
let shortcutInFlight = false;

export default function App() {
  const [route, setRoute] = useState<Route>("translation");
  const loadConfig = useSettingsStore((s) => s.loadConfig);
  const [bootError, setBootError] = useState<string | null>(null);

  // 启动时加载配置到全局 store
  // 阶段8：同步初始 pinned 状态为 config.pinnedByDefault
  useEffect(() => {
    getConfig()
      .then((cfg) => {
        loadConfig(cfg);
        // 同步 pinned：若 pinnedByDefault=true，初次触发即按固定窗口对待
        useTranslationStore.getState().setPinned(cfg.pinnedByDefault);
      })
      .catch((e) => {
        const err = toCommandError(e);
        setBootError(err.message);
      });
  }, [loadConfig]);

  // 监听托盘菜单事件：切换路由
  // Rust 端会先 emit 事件再显示窗口，前端据此切到对应页面
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let cancelled = false;

    Promise.all([
      listen("tray://settings", () => setRoute("settings")),
      listen("tray://show", () => setRoute("translation")),
      // 全局快捷键触发：捕获选中文本 → 调用翻译 → 显示窗口展示结果
      listen("shortcut://translate", () => {
        void handleShortcutTranslate();
      }),
    ]).then((fns) => {
      if (cancelled) {
        fns.forEach((f) => f());
        return;
      }
      unlisteners.push(...fns);
    });

    return () => {
      cancelled = true;
      unlisteners.forEach((f) => f());
    };
  }, []);

  // 全局快捷键触发的完整翻译流程：
  // 1. 切到翻译页 + 重置旧状态（保留 pinned） + 进入 capturing
  // 2. 阶段8：把窗口重新定位到鼠标附近（pinned=true 时跳过，但不抢焦点）
  // 3. captureSelectedText 在原应用仍有焦点时取得选中文本
  // 4. 显示窗口并获取焦点（让用户看到原文与翻译进度）
  // 5. translateText 调大模型翻译
  // 6. 按最新 requestId 写回结果，避免并发覆盖
  const handleShortcutTranslate = async () => {
    // 阶段9：防止重复请求。若上一次快捷键翻译仍在进行中，直接丢弃本次触发
    if (shortcutInFlight) {
      console.debug("[easyT] 上一次翻译仍在进行，丢弃本次快捷键触发");
      return;
    }
    shortcutInFlight = true;
    try {
      await runShortcutTranslate();
    } finally {
      shortcutInFlight = false;
    }
  };

  // 实际执行快捷键翻译流程（被 handleShortcutTranslate 包裹 in-flight 守卫）
  const runShortcutTranslate = async () => {
    setRoute("translation");
    const store = useTranslationStore.getState();
    store.reset();
    store.setStatus("capturing");

    const showWindow = async () => {
      try {
        const win = getCurrentWindow();
        await win.show();
        await win.setFocus();
      } catch (e) {
        console.warn("[easyT] 显示窗口失败:", e);
      }
    };

    // 阶段8：只移动窗口，不显示/聚焦，避免破坏原应用中的文本选区
    // pinned=true 时 Rust 端会跳过重新定位，保持当前位置
    try {
      await positionWindowNearMouse(store.pinned);
    } catch (e) {
      // 定位失败不阻断流程，仅记录
      console.warn("[easyT] 重新定位窗口失败:", e);
    }

    // 捕获选中文本
    let text: string;
    try {
      text = await captureSelectedText();
    } catch (e) {
      const err = toCommandError(e);
      await showWindow();
      const s = useTranslationStore.getState();
      s.setStatus("error");
      s.setError(err.message, err.kind);
      return;
    }

    await showWindow();

    // 调用翻译
    const { config } = useSettingsStore.getState();
    const requestId = store.startRequest(text);
    try {
      const result = await translateText({
        text,
        targetLanguage: config.targetLanguage,
      });
      // 仅最新请求可更新结果（防止并发覆盖）
      if (useTranslationStore.getState().requestId !== requestId) return;
      store.setTranslatedText(result.translatedText);
      store.setStatus("success");
    } catch (e) {
      if (useTranslationStore.getState().requestId !== requestId) return;
      const err = toCommandError(e);
      store.setStatus("error");
      store.setError(err.message, err.kind);
    }
  };

  // 监听窗口失焦：非固定状态下根据 autoHide 隐藏
  useEffect(() => {
    const win = getCurrentWindow();
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;

    win
      .onFocusChanged(({ payload: focused }) => {
        if (focused) return;
        // 失焦：检查 pinned 与 autoHide
        const { pinned } = useTranslationStore.getState();
        const { config } = useSettingsStore.getState();
        if (!pinned && config.autoHide) {
          win.hide().catch(() => {
            /* 忽略隐藏失败 */
          });
        }
      })
      .then((f) => {
        if (cancelled) {
          f();
          return;
        }
        unlisten = f;
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  if (route === "settings") {
    return (
      <div className="h-screen w-screen overflow-hidden bg-surface">
        <SettingsPage onBack={() => setRoute("translation")} />
      </div>
    );
  }

  return (
    <div className="h-screen w-screen overflow-hidden bg-surface">
      {bootError ? (
        <div className="px-3 py-2 text-xs text-danger">
          启动加载配置失败：{bootError}（请到设置页检查）
        </div>
      ) : null}
      <TranslationPage
        onOpenSettings={() => setRoute("settings")}
        onClose={() => {
          // 通过 Tauri 窗口 API 触发 close，会被 Rust 端拦截改为 hide
          getCurrentWindow().close().catch(() => {
            /* 忽略 */
          });
        }}
      />
    </div>
  );
}
