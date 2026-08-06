# Domain Docs

This repository uses a single domain context for easyT translation behavior.

## Before Exploring or Implementing

- Read the root `CONTEXT.md`.
- Read ADRs under `docs/adr/` that affect the area being changed.
- If an expected ADR directory does not exist, proceed without creating it; domain-modeling work creates documents only when a decision warrants them.

## Vocabulary

Use the canonical terms defined in `CONTEXT.md` in ticket titles, acceptance criteria, tests, user-facing documentation, and implementation reports. Do not substitute terms listed under `_Avoid_`.

For shortcut translation work, preserve the distinctions among `选区捕获`, `无选区`, `翻译请求`, and `显示恢复`.

## Decisions

If a proposed ticket or implementation conflicts with an ADR, report the conflict explicitly rather than silently overriding it.
