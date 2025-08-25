
# Contributing

This guide extends the root [CONTRIBUTING.md](/CONTRIBUTING.md) file with editor-specific information for Neovim integrations.

## Canonical Implementation

The Neovim Tinymist plugin serves as the **heavily-documented canonical implementation** of an editor language client for Tinymist. This means:

- **Reference Implementation**: Other editors should refer to this implementation for LSP client patterns, configuration handling, and event subscription mechanisms
- **Comprehensive Test Suite**: Complete spec coverage in `spec/` directory demonstrates expected behavior
- **Documentation**: Detailed [Specification.md](./Specification.md) documents all functionality and APIs
- **Development Patterns**: Well-established patterns for async testing, event handling, and project management

## Development Workflow

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

### Development Shell

```bash
./bootstrap.sh bash
```

Provides bash shell access in the test environment for debugging and exploration.

## Test Suite Structure

The `spec/` directory contains the comprehensive test suite:

- **`lsp_spec.lua`**: LSP client attachment and basic functionality
- **`export_spec.lua`**: PDF export on save functionality
- **`on_type_export_spec.lua`**: Real-time export while typing
- **`never_export_spec.lua`**: Disabled export behavior
- **`lockfile_spec.lua`**: Project resolution with lockfile database
- **`fixtures.lua`**: Test data and workspace setup
- **`helpers.lua`**: Test utilities and custom assertions
- **`main.py`**: Python test runner for headless execution

## Contributing Guidelines

When contributing to the Neovim plugin:

1. **Add tests first**: Write spec tests for new functionality
2. **Follow patterns**: Use existing test patterns from `helpers.lua` and `fixtures.lua`
3. **Update documentation**: Modify [Specification.md](./Specification.md) for API changes
4. **Test thoroughly**: Run `./bootstrap.sh test` to validate changes
5. **Consider other editors**: Remember this is the canonical implementation that others reference
