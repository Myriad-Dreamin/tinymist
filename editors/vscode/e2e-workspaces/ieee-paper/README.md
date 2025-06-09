
TODO: This is still in development. We have only integrated features to the server but not yet add exporter options to VSCode Tasks.

# Sample workspace to Make and Prepare for Submitting IEEE Papers

This workspace is designed to help you create and prepare IEEE papers using Typst. Hope this could help you get started with writing your paper in Typst until IEEE provides official support for Typst.

## How does it work?

It converts typst main file to *unstyled* body markup and PDF Figures by HTML Export. The `main.tex` then glues official IEEE templates together with the body markup to produce a final PDF that is ready for submission.

## Task Samples

See [Tasks](./.vscode/tasks.json) for a list of tasks that can be run in this workspace.

- "Export to LaTeX (IEEE)" - Exports the Typst document to LaTeX format.
- "Export to Word (IEEE)" - Exports the Typst document to Word format.
