import { useEffect, useRef, useState } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { TranslationPage } from "@/pages/TranslationPage";
import { SettingsPage } from "@/pages/SettingsPage";
import { useSettingsStore } from "@/stores/settingsStore";
import { useTranslationStore } from "@/stores/translationStore";
import { getConfig, toCommandError } from "@/services/tauriCommands";
import { startShortcutTranslation } from "@/services/translationCoordinator";

type Route = "translation" | "settings";

let suppressAutoHideUntil = 0;

const markDragRegionPointerDown = () => {
  suppressAutoHideUntil = Date.now() + 1500;
};

const markWindowResizeInteraction = () => {
  suppressAutoHideUntil = Date.now() + 1200;
};

export default function App() {
  const [route, setRoute] = useState<Route>("translation");
  const loadConfig = useSettingsStore((s) => s.loadConfig);
  const [bootError, setBootError] = useState<string | null>(null);
  // 同步读取最新路由：避免 effect 闭包中的陈旧 route 误判快捷键门控
  const routeRef = useRef(route);
  routeRef.current = route;

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
      // 全局快捷键触发：设置页打开时忽略，其余路由转发给协调器
      listen("shortcut://translate", () => {
        if (routeRef.current === "settings") return;
        startShortcutTranslation(setRoute);
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

  // 监听窗口失焦：非固定状态下根据 autoHide 隐藏
  useEffect(() => {
    const win = getCurrentWindow();
    let unlistenFocus: UnlistenFn | null = null;
    let unlistenResize: UnlistenFn | null = null;
    let cancelled = false;
    let focusedState = true;
    let deferredHideTimer: ReturnType<typeof setTimeout> | null = null;

    const clearDeferredHide = () => {
      if (deferredHideTimer) {
        clearTimeout(deferredHideTimer);
        deferredHideTimer = null;
      }
    };

    const hideIfAllowed = () => {
      if (focusedState) return;
      const { pinned } = useTranslationStore.getState();
      const { config } = useSettingsStore.getState();
      if (!pinned && config.autoHide) {
        win.hide().catch(() => {
          /* 忽略隐藏失败 */
        });
      }
    };

    const scheduleDeferredHide = (minimumDelayMs = 250) => {
      clearDeferredHide();
      const suppressionDelay = Math.max(suppressAutoHideUntil - Date.now(), 0);
      const delay = Math.max(minimumDelayMs, suppressionDelay + 50);
      deferredHideTimer = setTimeout(() => {
        deferredHideTimer = null;
        hideIfAllowed();
      }, delay);
    };

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest("[data-tauri-drag-region]")) {
        markDragRegionPointerDown();
      }
    };

    document.addEventListener("pointerdown", handlePointerDown, true);

    win
      .onFocusChanged(({ payload: focused }) => {
        focusedState = focused;
        if (focused) {
          clearDeferredHide();
          return;
        }
        if (Date.now() < suppressAutoHideUntil) {
          scheduleDeferredHide();
          return;
        }
        scheduleDeferredHide();
      })
      .then((f) => {
        if (cancelled) {
          f();
          return;
        }
        unlistenFocus = f;
      });

    win
      .onResized(() => {
        markWindowResizeInteraction();
        if (!focusedState) {
          scheduleDeferredHide();
        }
      })
      .then((f) => {
        if (cancelled) {
          f();
          return;
        }
        unlistenResize = f;
      });

    return () => {
      cancelled = true;
      clearDeferredHide();
      document.removeEventListener("pointerdown", handlePointerDown, true);
      unlistenFocus?.();
      unlistenResize?.();
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
          getCurrentWindow()
            .close()
            .catch(() => {
              /* 忽略 */
            });
        }}
      />
    </div>
  );
}
