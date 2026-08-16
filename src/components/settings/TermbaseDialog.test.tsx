import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { TermbaseDialog } from "./TermbaseDialog";
import { useTermbaseController } from "./useTermbaseController";
import { termEntry, termbaseSnapshot } from "./termbaseTestUtils";
import {
  createTermbaseEntry,
  deleteTermbaseEntry,
  getTermbase,
  setTermbaseEnabled,
  setTermbaseEntryEnabled,
  updateTermbaseEntry,
} from "@/services/tauriCommands";

vi.mock("@/services/tauriCommands", () => ({
  createTermbaseEntry: vi.fn(),
  deleteTermbaseEntry: vi.fn(),
  getTermbase: vi.fn(),
  setTermbaseEnabled: vi.fn(),
  setTermbaseEntryEnabled: vi.fn(),
  toCommandError: (error: unknown) => ({
    kind: "ConfigInvalid",
    message: error instanceof Error ? error.message : "操作失败",
  }),
  updateTermbaseEntry: vi.fn(),
}));

function Harness({
  open: initialOpen = true,
}: {
  open?: boolean;
} = {}) {
  const [open, setOpen] = useState(initialOpen);
  const controller = useTermbaseController();
  return (
    <>
      <button onClick={() => setOpen(true)}>管理术语表</button>
      <TermbaseDialog open={open} onClose={() => setOpen(false)} controller={controller} />
    </>
  );
}

describe("TermbaseDialog", () => {
  beforeEach(() => vi.clearAllMocks());

  it("lists entries after loading with language badge and master switch", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    render(<Harness />);

    expect(screen.getByText("正在读取术语表…")).toBeInTheDocument();
    expect(await screen.findByText("function")).toBeInTheDocument();
    expect(screen.getByText("→ 函数")).toBeInTheDocument();
    expect(screen.getByText("简体中文")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "启用术语表" })).toBeChecked();
    expect(screen.getByText(/已保存 1 条/)).toBeInTheDocument();
  });

  it("shows the one-shot storage warning banner", async () => {
    vi.mocked(getTermbase).mockResolvedValue({
      ...termbaseSnapshot([termEntry("1", "function", "函数")]),
      warning: {
        kind: "storageRecovered",
        message: "术语表存储文件已损坏，原文件已隔离并重建",
      },
    });
    render(<Harness />);
    expect(
      await screen.findByText(/术语表存储文件已损坏/),
    ).toBeInTheDocument();
  });

  it("shows a safe load failure state", async () => {
    vi.mocked(getTermbase).mockRejectedValue(new Error("无法读取术语表"));
    render(<Harness />);
    expect(
      await screen.findByText(/读取术语表失败：无法读取术语表/),
    ).toBeInTheDocument();
  });

  it("shows the empty state and search filters the list", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([
        termEntry("1", "neural network", "神经网络"),
        termEntry("2", "function", "函数"),
      ]),
    );
    render(<Harness />);
    expect(await screen.findByText("neural network")).toBeInTheDocument();
    expect(
      screen.queryByText("还没有术语条目，点击“新建术语”添加"),
    ).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("搜索术语"), {
      target: { value: "NEURAL" },
    });
    expect(screen.getByText("neural network")).toBeInTheDocument();
    expect(screen.queryByText("function")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("搜索术语"), {
      target: { value: "不存在" },
    });
    expect(screen.getByText("没有匹配的术语条目")).toBeInTheDocument();
  });

  it("paginates long lists at 20 entries per page", async () => {
    const entries = Array.from({ length: 25 }, (_, i) =>
      termEntry(String(i), `term-${i}`, `译-${i}`),
    );
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot(entries));
    render(<Harness />);

    expect(await screen.findByText("term-0")).toBeInTheDocument();
    expect(screen.queryByText("term-24")).not.toBeInTheDocument();
    expect(screen.getByText("第 1 / 2 页")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "下一页" }));
    expect(await screen.findByText("term-24")).toBeInTheDocument();
    expect(screen.queryByText("term-0")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "上一页" }));
    expect(await screen.findByText("term-0")).toBeInTheDocument();
  });

  it("keeps the term list in an independently scrollable region", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    render(<Harness />);

    await screen.findByText("function");
    expect(screen.getByRole("list", { name: "术语条目" })).toHaveClass(
      "max-h-48",
      "overflow-y-auto",
    );
  });

  it("toggles the master switch and per-entry switches", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    vi.mocked(setTermbaseEnabled).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")], false),
    );
    vi.mocked(setTermbaseEntryEnabled).mockResolvedValue(
      termbaseSnapshot([{ ...termEntry("1", "function", "函数"), enabled: false }]),
    );
    render(<Harness />);

    fireEvent.click(await screen.findByRole("switch", { name: "启用术语表" }));
    await waitFor(() =>
      expect(setTermbaseEnabled).toHaveBeenCalledWith(false),
    );

    fireEvent.click(screen.getByRole("switch", { name: "启用术语 function" }));
    await waitFor(() =>
      expect(setTermbaseEntryEnabled).toHaveBeenCalledWith("1", false),
    );
  });

  it("creates an entry through the inline editor and returns to the list", async () => {
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot([]));
    vi.mocked(createTermbaseEntry).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    render(<Harness />);

    fireEvent.click(
      await screen.findByRole("button", { name: /新建术语/ }),
    );
    expect(screen.getByLabelText("源术语")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("源术语"), {
      target: { value: "function" },
    });
    fireEvent.change(screen.getByLabelText("指定译法"), {
      target: { value: "函数" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(createTermbaseEntry).toHaveBeenCalledWith({
        sourceTerm: "function",
        targetLanguage: "简体中文",
        targetTerm: "函数",
        caseSensitive: false,
      }),
    );
    expect(await screen.findByText("function")).toBeInTheDocument();
  });

  it("disables save until both fields have content", async () => {
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot([]));
    render(<Harness />);

    fireEvent.click(await screen.findByRole("button", { name: /新建术语/ }));
    const save = screen.getByRole("button", { name: "保存" });
    expect(save).toBeDisabled();

    fireEvent.change(screen.getByLabelText("源术语"), {
      target: { value: "  function  " },
    });
    fireEvent.change(screen.getByLabelText("指定译法"), {
      target: { value: "  函数  " },
    });
    expect(save).toBeEnabled();
  });

  it("prefills the editor and updates an existing entry", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    vi.mocked(updateTermbaseEntry).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "功能")]),
    );
    render(<Harness />);

    fireEvent.click(
      await screen.findByRole("button", { name: "编辑术语 function" }),
    );
    expect(screen.getByLabelText("源术语")).toHaveValue("function");
    expect(screen.getByLabelText("指定译法")).toHaveValue("函数");

    fireEvent.change(screen.getByLabelText("指定译法"), {
      target: { value: "功能" },
    });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(updateTermbaseEntry).toHaveBeenCalledWith("1", {
        sourceTerm: "function",
        targetLanguage: "简体中文",
        targetTerm: "功能",
        caseSensitive: false,
      }),
    );
    expect(await screen.findByText("→ 功能")).toBeInTheDocument();
  });

  it("cancels editing without calling any mutation", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    render(<Harness />);

    fireEvent.click(
      await screen.findByRole("button", { name: "编辑术语 function" }),
    );
    fireEvent.click(screen.getByRole("button", { name: "取消" }));

    expect(screen.getByRole("button", { name: /新建术语/ })).toBeInTheDocument();
    expect(updateTermbaseEntry).not.toHaveBeenCalled();
  });

  it("keeps the confirmation open and pending until deletion succeeds", async () => {
    vi.mocked(getTermbase).mockResolvedValue(
      termbaseSnapshot([termEntry("1", "function", "函数")]),
    );
    let resolveDelete: (snapshot: ReturnType<typeof termbaseSnapshot>) => void;
    vi.mocked(deleteTermbaseEntry).mockImplementation(
      () => new Promise((resolve) => { resolveDelete = resolve; }),
    );
    render(<Harness />);

    fireEvent.click(
      await screen.findByRole("button", { name: "删除术语 function" }),
    );
    const confirmation = screen.getByRole("dialog", {
      name: "删除术语条目？",
    });
    expect(confirmation).toHaveTextContent("function");
    expect(screen.queryByRole("dialog", { name: "术语表" })).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "删除" }));
    await waitFor(() =>
      expect(deleteTermbaseEntry).toHaveBeenCalledWith("1"),
    );
    expect(screen.getByRole("dialog", { name: "删除术语条目？" })).toBeInTheDocument();
    expect(screen.getByRole("status", { name: "正在确认" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /正在确认 删除/ })).toBeDisabled();

    resolveDelete!(termbaseSnapshot([]));
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "删除术语条目？" })).not.toBeInTheDocument(),
    );
    expect(screen.queryByText("function")).not.toBeInTheDocument();
  });

  it("closes with Escape and resets the view state", async () => {
    vi.mocked(getTermbase).mockResolvedValue(termbaseSnapshot([]));
    render(<Harness />);

    fireEvent.click(await screen.findByRole("button", { name: /新建术语/ }));
    const dialog = screen.getByRole("dialog", { name: "术语表" });
    fireEvent(dialog, new Event("cancel", { cancelable: true }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "术语表" })).not.toBeInTheDocument(),
    );
  });
});
