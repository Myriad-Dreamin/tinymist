## 1. Align configuration model

- [x] 1.1 Update OpenSpec artifacts so the change describes configuration-wide restart-scoped client options rather than init-only session options.
- [x] 1.2 Add the missing editor-integration keys to runtime configuration polling.
- [x] 1.3 Keep configuration parsing unified so initialization options and runtime configuration refreshes update the same fields.

## 2. Apply changed client options consistently

- [x] 2.1 Add a focused restart-scoped client option diff that covers `compileStatus`, `triggerSuggest`, `triggerParameterHints`, `triggerSuggestAndParameterHints`, `supportHtmlInMarkdown`, `supportClientCodelens`, `supportExtendedCodeAction`, `customizedShowDocument`, and `delegateFsRequests`.
- [x] 2.2 Reload projects when that restart-scoped client option snapshot changes.
- [x] 2.3 Reset cached access models when `delegateFsRequests` changes before reloading projects.

## 3. Update editor integration and coverage

- [x] 3.1 Update the VS Code configuration middleware so requested injected client flags return their effective values during `workspace/configuration`.
- [x] 3.2 Add focused tests for config polling, runtime parsing of client option values, restart-scoped diffing, and VS Code configuration response synthesis.
- [x] 3.3 Run focused `tinymist` config tests and relevant VS Code tests, then review resulting fixture or snapshot changes.
