## 1. Report rejected settings from runtime synchronization

- [x] 1.1 Show the collected `Config::warnings` to the client at the end of
  `on_changed_configuration`, covering the `workspace/didChangeConfiguration` push path.
- [x] 1.2 Remove the duplicate display in `workspace_configuration_callback` so the pull path
  reports each rejected key exactly once.
- [x] 1.3 On the rollback path, report the rejection reason together with the refused update's
  warnings before restoring the old configuration.

## 2. Regression coverage

- [x] 2.1 Add tests asserting push-path payloads with a rejected value emit exactly one
  `window/showMessageRequest` (both top-level and `tinymist`-namespaced keys).
- [x] 2.2 Add tests asserting valid values emit none, the pull path emits exactly one, and a fully
  rejected update emits one message explaining the rejection without replaying old warnings.
