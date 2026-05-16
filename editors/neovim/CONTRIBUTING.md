
# Contributing

This guide extends the root [CONTRIBUTING.md](/CONTRIBUTING.md) file with editor-specific information for Neovim integrations.

## Canonical Implementation

The Neovim Tinymist plugin serves as the **heavily-documented canonical implementation** of an editor language client for Tinymist. This means:

- **Reference Implementation**: Other editors should refer to this implementation for LSP client patterns, configuration handling, and event subscription mechanisms
- **Comprehensive Test Suite**: Complete spec coverage in `spec/` directory demonstrates expected behavior
- **Documentation**: Detailed [Specification.md](./Specification.md) documents all functionality and APIs

## Development Workflow

### Spec Environment Notes

Neovim specs are container-only in normal development. Do not validate them
with a host `nvim`, because the spec runner expects the container layout,
runtime dependencies, and mounted fixtures used by `bootstrap.sh`.

Before editing or running specs, read this file together with
`docs/dev-guide.md`, then inspect the affected files under `spec/`, `lua/`,
and `scripts/minimal_init.lua`.

The supported entry point is:

```bash
./bootstrap.sh test
```

This builds/runs the `tinymist-nvim-spec-local` Neovim development container, mounts
`tests/workspaces` as `/home/runner/dev/workspaces`, runs
`tinymist compile` for the `book` fixture, then runs `spec/main.py` with
headless Neovim and the `inanis` runner. The container must provide the Lua
test dependencies on `/home/runner/packpath/*`, especially `inanis.nvim`,
`plenary.nvim`, and `nvim-lspconfig`.

The Neovim development image intentionally does not bundle `tinymist`.
`bootstrap.sh` mounts a host-built binary into the container as
`/usr/local/bin/tinymist`, preferring `target/debug/tinymist` and falling back
to `target/release/tinymist`. Build one first with `cargo build --bin tinymist`,
or set `TINYMIST_BIN=/absolute/path/to/tinymist` to override the selection.

If Docker builds need an HTTP proxy, do not use `127.0.0.1` inside the
container. Point Docker's proxy configuration at the host address visible from
the default bridge network, usually `172.17.0.1:<port>`. A temporary
`DOCKER_CONFIG` is preferred over editing the user's global Docker config.

Warnings from newer `nvim-lspconfig` about the deprecated
`require("lspconfig")` framework do not fail the suite by themselves. Treat a
zero exit status and the final `passed` summary as the test result. Remove
transient `.nvimlog` files if headless Neovim leaves them in the worktree.

### Interactive Editor Mode

```bash
./bootstrap.sh editor
```

Enters interactive edit mode for human experiencing the spec implementation. This launches Neovim in a Docker container with the plugin loaded, allowing you to:
- Test functionality manually
- Explore LSP features interactively
- Debug issues in a controlled environment
- Experience the canonical implementation first-hand

### Headless Testing

```bash
./bootstrap.sh test
```

Runs headless tests for automated validation. This executes the complete test suite including:
- LSP client attachment tests
- Export functionality validation  
- Project resolution testing
- Event subscription verification
- All spec files in `spec/*_spec.lua`

## Contributing Guidelines

When contributing to the Neovim plugin:

1. **Add tests first**: Write spec tests for new functionality
2. **Follow patterns**: Use existing test patterns from `helpers.lua` and `fixtures.lua`
3. **Update documentation**: Modify [Specification.md](./Specification.md) for API changes
4. **Test thoroughly**: Run `./bootstrap.sh test` to validate changes
5. **Add todo and skip** if asserts in test or spec file are not met. The todo is a reminder to fix the test in other PRs.
