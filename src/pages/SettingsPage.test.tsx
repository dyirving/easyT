import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { SettingsPage } from "./SettingsPage";

const mocks = vi.hoisted(() => ({
  useSettingsController: vi.fn(),
  useTermbaseController: vi.fn(),
}));

vi.mock("@/components/settings", () => ({
  CacheDetailsDialog: () => null,
  OfficialApiPanel: () => null,
  SettingsHeader: () => <header>设置</header>,
  SettingsRow: ({ title, description, control }: { title: string; description: string; control: React.ReactNode }) => (
    <section aria-label={title}><h2>{title}</h2><p>{description}</p>{control}</section>
  ),
  ShortcutInput: () => null,
  TermbaseDialog: ({ open }: { open: boolean }) => open ? <div role="dialog">术语表管理</div> : null,
  useSettingsController: mocks.useSettingsController,
  useTermbaseController: mocks.useTermbaseController,
  WebGatewayPanel: () => null,
}));

const config = {
  backendMode: "officialApi",
  targetLanguage: "简体中文",
  shortcut: "Ctrl+T",
  translationHistoryLimit: 5,
  timeoutSeconds: 60,
  maxTextLength: 5000,
  enableThinking: false,
  streamOutput: false,
  autoHide: false,
  pinnedByDefault: false,
  provider: "agnes",
  model: "agnes-2.0-flash",
  apiKey: "",
  baseUrl: "",
  webGateway: { provider: "qwen", saveHistory: false },
};

describe("SettingsPage termbase entry", () => {
  it("shows the persisted count and recovery warning, then opens management", () => {
    mocks.useSettingsController.mockReturnValue({
      config,
      loadingConfig: false,
      loadError: null,
      isWebGateway: false,
      setConfig: vi.fn(),
      changeBackend: vi.fn(),
      changeProvider: vi.fn(),
      changeApiKey: vi.fn(),
      historyLimitInput: "5",
      historyLimitError: undefined,
      changeHistoryLimit: vi.fn(),
      saving: false,
      save: vi.fn(),
      testing: "idle",
      test: vi.fn(),
      testMessage: null,
      saveMessage: null,
      saveError: false,
      saveWarning: null,
      qwenAccountPool: null,
      qwenAccountPending: false,
      qwenAccountError: null,
      createQwenAccount: vi.fn(),
      beginQwenAccountLogin: vi.fn(),
      renameQwenAccount: vi.fn(),
      setQwenAccountEnabled: vi.fn(),
      moveQwenAccount: vi.fn(),
      testQwenAccount: vi.fn(),
      setAccountDestructiveIntent: vi.fn(),
      accountDestructiveIntent: null,
      confirmAccountDestructiveAction: vi.fn(),
    });
    mocks.useTermbaseController.mockReturnValue({
      phase: "ready",
      totalCount: 2,
      snapshot: {
        enabled: false,
        entries: [],
        maximumEntries: 200,
        warning: { kind: "storageRecovered", message: "术语表已恢复" },
      },
    });

    render(<SettingsPage onBack={vi.fn()} />);

    expect(screen.getByRole("region", { name: "术语表" })).toHaveTextContent("已保存 2 条术语");
    expect(screen.getByText("术语表已恢复")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "管理术语表" }));
    expect(screen.getByRole("dialog")).toHaveTextContent("术语表管理");
  });
});
