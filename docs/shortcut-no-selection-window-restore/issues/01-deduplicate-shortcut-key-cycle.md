# 01 — 按键周期内只激活一次快捷键

**What to build:** 让当前配置的全局翻译快捷键在一次完整的按下到释放周期内最多激活一次选区捕获。用户长按快捷键时不会产生重复激活，释放后再次按下仍可正常使用；快捷键被替换后，旧快捷键或残留按下状态不能触发翻译。

**Blocked by:** None — can start immediately. Implementation still requires the source SDD to be explicitly approved.

**Status:** ready-for-agent

**Source:** `docs/SDD-shortcut-no-selection-window-restore.md`，重点覆盖 FR-006、NFR-003、T-001 至 T-004 和 M-007。

- [ ] 当前配置快捷键的首次按下只产生一次 `shortcut://translate` 激活，事件名称和负载保持兼容。
- [ ] 同一按下到释放周期内的重复按下事件不会产生额外激活。
- [ ] 收到释放事件后，下一次按下能够产生新的激活。
- [ ] 已替换、待清理或非当前快捷键的事件不会触发翻译。
- [ ] 快捷键替换会清除不再有效的按下状态，不会让新快捷键永久失效。
- [ ] 自动化测试覆盖首次按下、重复按下、释放后再按、非当前快捷键和快捷键替换场景。
- [ ] Rust 定向测试、完整测试和格式检查通过，并记录命令证据。
