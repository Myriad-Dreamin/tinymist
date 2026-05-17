## Context

When Tinymist runs with delegated client-side watching, the server asks the client to watch paths through `tinymist/fs/watch`. The VS Code extension reads file contents and sends `tinymist/fsChange` requests with insert and remove payloads. This path differs from server-side `notify-rs`: delete currently sends a read-error style insert unless explicit removes are sent, rename often appears as delete/create, and a watch clock filters stale reads.

The formal operation model should therefore be tested at this ingress boundary. The goal is not to prove every LSP API again, but to prove that delegated watch events normalize to the same workspace-change obligations as server-side events.

## Goals / Non-Goals

**Goals:**
- Test delegated watch bookkeeping in the VS Code extension: watched set, read clock, sync reads, deletes, unwatch, and stale-read suppression.
- Test `tinymist/fsChange` request handling in the tinymist server with insert, remove, read-error, and sync/non-sync payloads.
- Map delegated client watch behavior to `O01..O20` rows where client-side watching is a valid ingress path.
- Assert representative server-visible effects and LSP response transitions for delegated changes.

**Non-Goals:**
- Testing raw `notify-rs` server-side watcher behavior.
- Replacing tinymist LSP workspace-change API coverage.
- Testing all editor UI workflows or VS Code APIs with a real extension host in the first pass.
- Changing the delegated watch protocol unless tests expose an unavoidable protocol gap.

## Decisions

### 1. Split client bookkeeping tests from server request tests

TypeScript tests should cover `editors/vscode/src/lsp.ts` behavior around `watches`, `hasRead`, watch clock ordering, sync reads, unwatch, and sendRequest payload shape. Rust tests should cover `tinymist/fsChange` handling and its effect on server/project state.

Alternative considered: only run end-to-end VS Code tests. Rejected because extension-host tests are slower and make it harder to isolate whether a failure is client bookkeeping or server normalization.

### 2. Treat delegated rename as normalized delete/create unless protocol support changes

The current delegated watcher receives create/change/delete notifications and file reads. A filesystem rename should be represented as remove old plus insert new, or as read-error old plus ok new, depending on what the client can observe. Tests should document this normalization explicitly.

Alternative considered: require paired rename metadata from the client. Rejected for this proposal because that would be a protocol change, not just coverage.

### 3. Compare delegated ingress against normalized workspace obligations

The test matrix should not assert exact server-side watcher raw events. It should assert that delegated inputs lead to the same final obligations: old depended paths retire, new referenced paths become readable, read errors replace stale content, sync reads obey ordering, and unrelated churn stays harmless.

Alternative considered: duplicate notify actor expected event shapes. Rejected because delegated watch operates at a different boundary.

### 4. Include a small LSP-facing smoke layer

After server `tinymist/fsChange` normalization, selected tests should invoke a representative LSP query or diagnostics wait point to prove the server state is usable. Detailed API response transition coverage remains in the separate tinymist LSP proposal.

Alternative considered: stop at request deserialization. Rejected because correct payload shape alone does not prove workspace state changed.

## Risks / Trade-offs

- [Client watch internals are currently local to one method] -> Extract small testable helpers only if needed, keeping production behavior stable.
- [VS Code filesystem behavior can be hard to unit test] -> Unit-test pure bookkeeping and mock `workspace.fs.readFile`; reserve real extension-host tests for later if needed.
- [Delete semantics may reveal protocol ambiguity] -> Document current read-error/remove behavior and open a follow-up protocol proposal if explicit delete payloads are insufficient.
- [Overlap with LSP matrix] -> Keep only representative LSP smoke checks here; full API transition coverage belongs to `test-tinymist-lsp-workspace-change-matrix`.

## Migration Plan

1. Add client-side delegated watch unit tests or extract a testable helper for watch bookkeeping.
2. Add Rust tests for `tinymist/fsChange` request handling and normalized server/project state.
3. Map delegated ingress coverage to operation rows and document unsupported rows.
4. Add representative LSP smoke assertions for key delegated changes.
5. Validate with focused TypeScript and Rust test commands.

## Open Questions

- Whether the VS Code watcher bookkeeping should be extracted into a small module to avoid testing through the whole `Tinymist` class.
- Whether delegated delete should send explicit removes in more cases or continue relying on read-error inserts.
