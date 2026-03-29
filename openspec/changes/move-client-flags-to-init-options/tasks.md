## 1. Split session-scoped options from runtime config

- [ ] 1.1 Introduce a dedicated initialize-time parsing path for session-scoped client options, including `compileStatus` and the client capability flags currently injected by shipped editors.
- [ ] 1.2 Remove session-scoped keys from `workspace/configuration` polling and make `didChangeConfiguration` ignore those keys if a client still sends them at runtime.
- [ ] 1.3 Keep server consumers such as code-lens behavior, compile-status notifications, and related editor/export wiring driven by the captured session-scoped values for the whole LSP session.

## 2. Update integrations, fixtures, and docs

- [ ] 2.1 Update shipped editor integrations and initialization fixtures so session-scoped values are supplied through `initializationOptions` instead of being treated as ordinary runtime workspace settings.
- [ ] 2.2 Update config docs or editor-facing wording so `compileStatus` is clearly described as session-scoped and any runtime changes are framed as taking effect on the next server session.
- [ ] 2.3 Keep non-user client capability flags out of user-facing workspace-config surfaces where they do not belong.

## 3. Add regression coverage

- [ ] 3.1 Add focused tests for config parsing that prove session-scoped options survive later config refreshes that omit those keys.
- [ ] 3.2 Add or adjust LSP coverage showing `supportClientCodelens` and `compileStatus` behave according to initialize-time options for the lifetime of the session.
- [ ] 3.3 Run focused `tinymist` config/LSP tests and the relevant smoke coverage, then review the resulting fixture or snapshot changes.
