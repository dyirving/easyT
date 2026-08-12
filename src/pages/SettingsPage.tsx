import { Database } from "lucide-react";
import { useState } from "react";
import { ConfirmDialog, StatusBanner } from "@/components/patterns";
import {
  CacheDetailsDialog,
  OfficialApiPanel,
  SettingsHeader,
  SettingsRow,
  ShortcutInput,
  useSettingsController,
  WebGatewayPanel,
} from "@/components/settings";
import {
  Button,
  Combobox,
  FormField,
  Input,
  Select,
  Spinner,
  Switch,
} from "@/components/ui";
import { TARGET_LANGUAGES, type BackendMode } from "@/types";

const HISTORY_LIMIT_OPTIONS = [5, 10, 15, 20].map((value) => ({
  value: String(value),
  label: `${value} 条`,
}));

export function SettingsPage({
  onBack,
  onCacheCleared,
}: {
  onBack: () => void;
  onCacheCleared?: () => void;
}) {
  const controller = useSettingsController();
  const [cacheOpen, setCacheOpen] = useState(false);
  return (
    <div className="flex h-full flex-col">
      <SettingsHeader onBack={onBack} />
      <main className="flex-1 overflow-y-auto px-4 py-4">
        <div className="mx-auto max-w-xl space-y-5">
          {controller.loadingConfig ? (
            <p className="flex gap-2 text-sm text-ink-muted">
              <Spinner />正在加载配置…
            </p>
          ) : null}
          {controller.loadError ? (
            <StatusBanner
              tone="danger"
              announcement="assertive"
              description={`加载配置失败：${controller.loadError}（将使用默认配置）`}
            />
          ) : null}
          <FormField
            label="翻译后端"
            hint="Official API 使用付费接口；WebGateway 为实验功能"
          >
            <Select
              value={controller.config.backendMode}
              onChange={(event) =>
                controller.changeBackend(event.target.value as BackendMode)
              }
            >
              <option value="officialApi">Official API（付费）</option>
              <option value="webGateway">Qwen 网页实验模式</option>
            </Select>
          </FormField>
          {controller.isWebGateway ? (
            <WebGatewayPanel
              config={controller.config}
              setConfig={controller.setConfig}
              status={controller.loginStatus}
              pending={controller.loginActionPending}
              onLogin={() => void controller.beginLogin()}
              onLogout={() => controller.setLogoutIntent(true)}
            />
          ) : (
            <OfficialApiPanel
              config={controller.config}
              setConfig={controller.setConfig}
              onProviderChange={controller.changeProvider}
              onApiKeyChange={controller.changeApiKey}
            />
          )}
          <FormField
            label="全局快捷键"
            hint="有选区时翻译，无选区时显示翻译窗口；默认 Ctrl+T，可能与其他软件冲突"
          >
            <ShortcutInput
              value={controller.config.shortcut}
              onChange={(shortcut) => controller.setConfig({ shortcut })}
            />
          </FormField>
          <FormField label="目标语言">
            <Select
              value={controller.config.targetLanguage}
              onChange={(event) =>
                controller.setConfig({ targetLanguage: event.target.value })
              }
            >
              {TARGET_LANGUAGES.map((item) => (
                <option key={item.value} value={item.value}>
                  {item.label}
                </option>
              ))}
            </Select>
          </FormField>
          <FormField
            label="最多保留翻译历史"
            hint="预设 5、10、15、20；也可输入 1～20。减少数量会立即移除超限的较早记录。"
            error={controller.historyLimitError}
          >
            <Combobox
              value={controller.historyLimitInput}
              options={HISTORY_LIMIT_OPTIONS}
              onValueChange={controller.changeHistoryLimit}
              inputMode="numeric"
              required
            />
          </FormField>
          <div className="grid grid-cols-1 gap-4 min-[460px]:grid-cols-2">
            <FormField label="请求超时（秒）" hint="5～300">
              <Input
                type="number"
                min={5}
                max={300}
                value={controller.config.timeoutSeconds}
                onChange={(event) =>
                  controller.setConfig({
                    timeoutSeconds: Number(event.target.value) || 60,
                  })
                }
              />
            </FormField>
            <FormField label="最大翻译字符数" hint="默认 5000">
              <Input
                type="number"
                min={100}
                max={20000}
                value={controller.config.maxTextLength}
                onChange={(event) =>
                  controller.setConfig({
                    maxTextLength: Number(event.target.value) || 5000,
                  })
                }
              />
            </FormField>
          </div>
          <div className="space-y-3 rounded-lg border border-line bg-surface-soft/40 px-3 py-3">
            <SettingsRow
              title="启用思考模式"
              description="关闭（默认）省 token、更快；开启可在复杂语境下提升译文质量"
              control={
                <Switch
                  checked={controller.config.enableThinking}
                  onCheckedChange={(enableThinking) =>
                    controller.setConfig({ enableThinking })
                  }
                  aria-label="启用思考模式"
                />
              }
            />
            <SettingsRow
              title="流式输出"
              description="生成时逐步显示译文；Official API 端点需支持标准流式响应"
              control={
                <Switch
                  checked={controller.config.streamOutput}
                  onCheckedChange={(streamOutput) =>
                    controller.setConfig({ streamOutput })
                  }
                  aria-label="流式输出"
                />
              }
            />
            <SettingsRow
              title="自动隐藏窗口"
              description="失去焦点后隐藏临时窗口"
              control={
                <Switch
                  checked={controller.config.autoHide}
                  onCheckedChange={(autoHide) =>
                    controller.setConfig({ autoHide })
                  }
                  aria-label="自动隐藏窗口"
                />
              }
            />
            <SettingsRow
              title="默认常驻窗口"
              description="首次触发时即固定窗口"
              control={
                <Switch
                  checked={controller.config.pinnedByDefault}
                  onCheckedChange={(pinnedByDefault) =>
                    controller.setConfig({ pinnedByDefault })
                  }
                  aria-label="默认常驻窗口"
                />
              }
            />
          </div>
          <SettingsRow
            title="翻译缓存"
            description="查看本机缓存的条目、磁盘占用、命中率和保存位置"
            control={
              <Button variant="outline" onClick={() => setCacheOpen(true)}>
                <Database />查看缓存详情
              </Button>
            }
          />
          <div className="flex gap-2">
            <Button
              variant="primary"
              onClick={() => void controller.save()}
              loading={controller.saving}
              disabled={Boolean(controller.historyLimitError)}
            >
              保存
            </Button>
            <Button
              variant="outline"
              onClick={() => void controller.test()}
              loading={controller.testing === "testing"}
            >
              测试连接
            </Button>
          </div>
          {controller.testMessage ? (
            <StatusBanner
              tone={controller.testing === "ok" ? "success" : "danger"}
              announcement="polite"
              description={controller.testMessage}
            />
          ) : null}
          {controller.saveMessage ? (
            <StatusBanner
              tone={controller.saveError ? "danger" : "success"}
              announcement="polite"
              description={controller.saveMessage}
            />
          ) : null}
          {controller.saveWarning ? (
            <StatusBanner
              tone="warning"
              announcement="polite"
              description={controller.saveWarning}
            />
          ) : null}
        </div>
      </main>
      <CacheDetailsDialog
        open={cacheOpen}
        onClose={() => setCacheOpen(false)}
        onCacheCleared={onCacheCleared}
      />
      <ConfirmDialog
        open={controller.logoutIntent}
        title="确定退出 Qwen 登录？"
        description="退出后会删除本地保存的登录凭证和 Qwen 浏览器 profile，直到重新登录前翻译不可用。"
        confirmLabel="退出登录"
        cancelLabel="取消"
        tone="danger"
        pending={controller.loginActionPending}
        onCancel={() => controller.setLogoutIntent(false)}
        onConfirm={() => void controller.logout()}
      />
    </div>
  );
}
