# Issue Tracker: Local Markdown

Issues for this repository live as Markdown files under `docs/`, grouped by feature.

## Conventions

- One feature per directory: `docs/<feature-slug>/`.
- A feature's implementation tickets live at `docs/<feature-slug>/issues/<NN>-<slug>.md`.
- Number tickets from `01` in dependency order, with blockers before blocked tickets.
- Keep one ticket per file; never combine all tickets into one file.
- A feature SDD or specification may live inside its feature directory or at `docs/SDD-<feature-slug>.md`. Tickets must link to the canonical source document.
- Record triage state as a `Status:` line near the top of each ticket, using `docs/agents/triage-labels.md`.
- Declare blocking edges using ticket numbers and titles. A ticket is unblocked when every listed blocker is complete.
- Append discussion history under a `## Comments` heading when comments need to be retained.

## Publishing

When a skill says to publish to the issue tracker, create one file per ticket under the feature's `issues/` directory. Create the feature and issue directories when needed.

## Fetching

When a skill says to fetch a ticket, read the referenced ticket file in full, including comments. If only a ticket number is given, resolve it within the active feature directory.

## Frontier

The frontier consists of tickets with `Status: ready-for-agent` whose blocking tickets are all complete. Work the lowest-numbered available frontier ticket first unless the user directs otherwise.
