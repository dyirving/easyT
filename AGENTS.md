# Agent Instructions

## Agent skills

### Issue tracker

Issues are tracked as local Markdown files under `docs/<feature-slug>/issues/`. See `docs/agents/issue-tracker.md`.

### Triage labels

Use the canonical `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, and `wontfix` roles. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repository. Read the root `CONTEXT.md` and relevant ADRs before planning or implementation. See `docs/agents/domain.md`.

### Frontend UI Kit

Before planning or implementing any change to a frontend page, UI module, or frontend style, read `docs/UI-Kit需求与架构共识文档.md` completely.

- Reuse the existing UI Kit modules and design tokens before adding page-local UI implementation.
- Do not reimplement behavior or styles already owned by Button, IconButton, Input, Textarea, Select, Switch, FormField, Dialog, Spinner, StatusBanner, or ConfirmDialog.
- Do not add actual color values, duplicate control recipes, page-local modal/focus management, or `window.confirm`.
- If a capability is missing, extend the UI Kit when at least two domains will reuse it or when it hides substantial interaction/accessibility complexity. Keep single-domain behavior in its domain directory.
- Do not create shallow wrappers merely to remove a short class list. Native semantic layout elements remain allowed.
- Preserve the easyT visual language, accessibility requirements, import seams, test rules, and bundle budget defined by the canonical UI Kit document.
- Any new UI dependency requires explicit project-owner approval.
