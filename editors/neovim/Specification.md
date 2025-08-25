# Neovim Tinymist Plugin Specification

This document describes the comprehensive specification for the Neovim Tinymist plugin, which provides Language Server Protocol (LSP) integration for Typst documents. This plugin serves as the heavily-documented canonical implementation of an editor language client for Tinymist.

## Overview

The Tinymist Neovim plugin provides integrated Typst language support through:
- LSP client functionality with auto-attachment
- Export capabilities (PDF generation)
- Project resolution modes
- Development event subscriptions
- Comprehensive test suite

## Test Suite Structure

### Core Test Files

#### `spec/lsp_spec.lua`
Tests basic LSP client attachment functionality:
- **Auto-attachment to .typ files within projects**: Validates LSP client attaches when opening Typst files in project directories
- **Auto-attachment to single .typ files**: Ensures LSP works for standalone Typst files
- **Non-attachment to non-Typst files**: Confirms LSP doesn't attach to non-Typst file types

#### `spec/export_spec.lua` 
Tests export functionality with `exportPdf = 'onSave'`:
- **No PDF creation without save**: Verifies PDF isn't generated when typing without saving
- Tests export behavior when `systemFonts = false`

#### `spec/on_type_export_spec.lua`
Tests real-time export functionality with `exportPdf = 'onType'`:
- **PDF creation on typing**: Validates PDF generation as user types
- **Development event subscription**: Tests event-driven export notifications
- Uses async test framework for event handling

#### `spec/never_export_spec.lua`
Tests disabled export functionality:
- **No PDF creation ever**: Ensures no PDF generation when exports are disabled

#### `spec/lockfile_spec.lua`
Tests project resolution with lockfile database:
- **Project resolution mode `lockDatabase`**: Tests lockfile-based project management
- **Main file PDF creation**: Validates correct main file identification for export
- **Nested file handling**: Ensures nested files export to main project PDF

#### `spec/fixtures.lua`
Provides test fixture management:
- **Project structure setup**: Defines test workspace with book project
- **File existence validation**: Provides existing/non-existing file fixtures
- **Test environment configuration**: Sets up consistent test data

#### `spec/helpers.lua`
Provides test helper utilities:
- **LSP client management**: Functions for client attachment testing
- **Buffer operations**: Text insertion, cursor movement, search functionality
- **Event handling**: LSP ready state checking, diagnostic waiting
- **Test assertions**: Custom assertions for buffer contents, cursor position, etc.

### Test Infrastructure

#### `spec/main.py`
Python test runner that:
- **Prepares test environment**: Compiles book workspace using tinymist
- **Runs Neovim headlessly**: Executes test suite without GUI
- **Manages test files**: Discovers and runs all `*_spec.lua` files
- **Uses minimal init**: Loads minimal Neovim configuration for testing

## LSP Configuration Options

The plugin supports extensive LSP configuration through `init_options`:

### Export Configuration
- **`exportPdf`**: Controls PDF export behavior
  - `"onSave"`: Export when file is saved
  - `"onType"`: Export while typing
  - `"never"`: Disable exports
- **`outputPath`**: Destination path for exported files (supports `$name` variable)

### Project Resolution
- **`projectResolution`**: Determines project management mode
  - `"singleFile"`: Each file is independent document
  - `"lockDatabase"`: Workspace-based project with lock file tracking

### Development Features
- **`development`**: Enables development mode features
- **`systemFonts`**: Controls system font usage (often disabled in tests)

### Completion Features
- **`completion.postfix`**: Enable postfix completion
- **`completion.postfixUfcs`**: Enable UFCS-style completion
- **`completion.symbol`**: Symbol completion configuration

### Other Options
- **`compileStatus`**: Compilation status reporting
- **`formatterMode`**: Code formatting configuration
- **`semanticTokens`**: Semantic highlighting
- **`lint.enabled`**: Linting functionality

## Plugin Architecture

### Core Components

#### `lua/tinymist/init.lua`
Main plugin entry point:
- **Setup function**: Configures plugin with user options
- **Event subscription**: Provides development event callback registration
- **LSP integration**: Delegates to LSP module for client setup

#### `lua/tinymist/lsp.lua`
LSP client management:
- **Client configuration**: Sets up LSP client with capabilities and handlers
- **Event handling**: Processes `tinymist/devEvent` notifications
- **Development events**: Manages export and compilation event subscriptions

### Test Framework Integration

The plugin uses:
- **inanis**: Neovim test framework for spec execution
- **plenary.async**: Async testing utilities for event-driven tests
- **luassert**: Assertion library with custom assertions
- **minimal_init.lua**: Minimal Neovim configuration for testing

## Bootstrap Script Usage

The `bootstrap.sh` script provides development workflow commands:

### Interactive Editor Mode
```bash
./bootstrap.sh editor
```
Launches Neovim in Docker container for experiencing the spec implementation interactively.

### Headless Test Mode  
```bash
./bootstrap.sh test
```
Runs complete test suite headlessly for automated validation.

### Development Shell
```bash
./bootstrap.sh bash
```
Provides bash shell access in the test environment.

## Development Workflow

### Test-Driven Development
1. **Write specifications**: Add test cases in `spec/*_spec.lua` files
2. **Implement features**: Update plugin code to pass tests
3. **Validate behavior**: Run tests to ensure correctness
4. **Document changes**: Update specification as features evolve

### Testing Best Practices
- **Use fixtures**: Leverage `spec/fixtures.lua` for consistent test data
- **Helper functions**: Utilize `spec/helpers.lua` for common operations
- **Async patterns**: Use async tests for event-driven functionality
- **Isolation**: Ensure tests don't interfere with each other

## API Reference

### Setup Function
```lua
require('tinymist').setup({
  lsp = {
    init_options = {
      exportPdf = 'onType',
      projectResolution = 'lockDatabase',
      -- ... other options
    }
  }
})
```

### Event Subscription
```lua
require('tinymist').subscribeDevEvent(function(result)
  if result.type == 'export' and result.needExport then
    -- Handle export event
    return true -- Unregister callback
  end
end)
```

## Canonical Implementation Status

This Neovim plugin serves as the **canonical implementation** of a Tinymist editor language client, providing:

- **Complete test coverage**: Comprehensive spec suite covering all major functionality
- **Reference implementation**: Well-documented patterns for other editors
- **Event-driven architecture**: Proper LSP event handling and subscription
- **Project management**: Support for both single-file and project-based workflows
- **Export functionality**: Full PDF export capabilities with multiple trigger modes

Other editor integrations should refer to this implementation for:
- LSP client setup patterns
- Configuration option handling
- Event subscription mechanisms
- Test suite structure and coverage