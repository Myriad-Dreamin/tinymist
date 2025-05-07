#import "mod.typ": *

#show: book-page.with(title: [Linting Feature])

The linting feature is available since `tinymist` v0.13.12.

If enabled, the linter will run on save or on type, depending on your configuration. When it finishes, the language server will send the results along with the compilation diagnostics to the editor.

=== Configuring in VS Code
+ Open settings.
+ Search for "Tinymist Lint" and modify the value.
  + Toggle "Enabled" to enable or disable the linter.
  + Change "When" to configure when the linter runs.
    - (Default) `onSave` run linting when you save the file.
    - `onType` run linting as you type.

=== Configuring in Other Editors

+ Change configuration `tinymist.lint.enabled` to `true` to enable the linter.
+ Change configuration `tinymist.lint.when` to `onSave` or `onType` to configure when the linter runs.
  - (Default) `onSave` run linting when you save the file.
  - `onType` run linting as you type.


