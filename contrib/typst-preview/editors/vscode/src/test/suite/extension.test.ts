import * as assert from 'assert';

// You can import and use all API from the 'vscode' module
// as well as import your extension to test it
import * as vscode from 'vscode';
import * as ext from '../../extension';

const jsonIs = (pred: (x: string, y: string) => void) => (x: unknown, y: unknown) =>
  pred(JSON.stringify(x), JSON.stringify(y));

suite('Extension Test Suite', () => {
	vscode.window.showInformationMessage('Start all tests.');

	test('Sample test', () => {
		assert.strictEqual(-1, [1, 2, 3].indexOf(5));
		assert.strictEqual(-1, [1, 2, 3].indexOf(0));
	});

	test('Executable Configuration Test', async () => {
		assert.strictEqual('', vscode.workspace.getConfiguration().get<string>('typst-preview.executable'), 'default path');
    
    assert.notStrictEqual('', await ext.getCliPath(), 'never resolve empty string');
    assert.notStrictEqual(undefined, await ext.getCliPath(), 'never resolve undefined');

    const state = ext.getCliPath as unknown as any;
    let resolved: string;

    const BINARY_NAME = state.BINARY_NAME;
    assert.strictEqual('typst-preview', BINARY_NAME, 'default binary path is typst-preview');

    resolved = await ext.getCliPath();
    assert.strictEqual(state.bundledPath, resolved, 'the bundle path exists and detected');

    state.BINARY_NAME = 'bad-typst-preview';
    assert.strictEqual('bad-typst-preview', await ext.getCliPath(), 'fallback to binary name if not exists');

    const oldGetConfig = state.getConfig;
    state.getConfig = () => 'config-typst-preview';
    assert.strictEqual('config-typst-preview', await ext.getCliPath(), 'use config if set');
    
    state.BINARY_NAME = 'typst-preview';
    state.getConfig = oldGetConfig;
    resolved = await ext.getCliPath();
    assert.strictEqual(state.bundledPath, resolved, 'reactive state');

    resolved = await ext.getCliPath();
    assert.strictEqual(true, resolved.endsWith(state.BINARY_NAME), 'exact file suffix');

    /// fast path should hit
    for (let i = 0; i < 1000; i++) {
      await ext.getCliPath();
    }
  });

  test("Font Configuration Test", async () => {

    /// check default not ignore system fonts
    jsonIs(assert.strictEqual)(
      false,
      vscode.workspace.getConfiguration().get<boolean>("typst-preview.ignoreSystemFonts")
    );

    /// check that default font paths should be []
    jsonIs(assert.strictEqual)(
      [],
      vscode.workspace.getConfiguration().get<string[]>("typst-preview.fontPaths")
    );

    jsonIs(assert.strictEqual)(
      [], ext.getCliFontPathArgs(undefined));

    jsonIs(assert.strictEqual)(
      [], ext.getCliFontPathArgs([]));

    jsonIs(assert.strictEqual)(
      [], ext.codeGetCliFontArgs());

    jsonIs(assert.strictEqual)(
      ["--font-path", "/path/to/font1", "--font-path", "/path/to/font2"],
      ext.getCliFontPathArgs(["/path/to/font1", "/path/to/font2"]));
  });
});
