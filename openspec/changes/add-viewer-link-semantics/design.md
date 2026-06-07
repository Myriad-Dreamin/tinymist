## Context

`tinymist-viewer` converts Reflexo vector pages into `VecScene` and then flushes them into Vello scenes. The rendering stack currently implements drawing hooks for paths, images, items, and glyphs, but semantic hooks such as `render_link` are no-ops. The UI canvas receives only page clicks and sends them to the preview server as source-position sync requests.

## Goals / Non-Goals

**Goals:**

- Preserve link hit rectangles from vector IR while keeping drawing and semantics separate.
- Open supported external link schemes from page clicks.
- Keep source-position sync unchanged for clicks outside supported links.

**Non-Goals:**

- Support internal Typst destinations, file paths, or relative links in the first pass.
- Render or embed arbitrary HTML.
- Build a full accessibility tree for text semantics.

## Decisions

- Store semantic links as sidecar page data, not as `VecScene` variants.
  - Rationale: links are interactive metadata, not paint commands. Keeping them separate avoids complicating Vello rendering and lets click handling remain explicit.

- Convert link boxes into page coordinates while constructing `RenderStack`.
  - Rationale: the click path already converts canvas coordinates back into page coordinates, so hit-testing can be independent of the current zoom.

- Use the existing workspace `open` crate.
  - Rationale: the workspace already pins and uses `open::that_detached` for preview/export opening. Reusing it avoids a new dependency family.

- Allow only `http`, `https`, and `mailto`.
  - Rationale: these are expected external link schemes. `file`, relative paths, and internal destinations need workspace/root policy or preview-protocol support and should not be opened implicitly.

## Risks / Trade-offs

- [Transformed link rectangles can be rotated or skewed] -> Store the transformed bounding box for first-pass hit-testing; this may over-approximate rotated links but is robust and simple.
- [Overlapping links need deterministic handling] -> Hit-test links in reverse semantic order so later/topmost items win.
- [System open failures are platform-specific] -> Log open failures and keep the preview task running.
