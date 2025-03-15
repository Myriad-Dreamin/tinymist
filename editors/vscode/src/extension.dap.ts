/*---------------------------------------------------------
 * Copyright (C) Microsoft Corporation. All rights reserved.
 *--------------------------------------------------------*/
/*
 * extension.ts (and activateMockDebug.ts) forms the "plugin" that plugs into VS Code and contains the code that
 * connects VS Code with the debug adapter.
 *
 * extension.ts contains code for launching the debug adapter in three different ways:
 * - as an external program communicating with VS Code via stdin/stdout,
 * - as a server process communicating with VS Code via sockets or named pipes, or
 * - as inlined code running in the extension itself (default).
 *
 * Since the code in extension.ts uses node.js APIs it cannot run in the browser.
 */

"use strict";

import * as Net from "net";
import * as vscode from "vscode";
import { randomBytes } from "crypto";
import { tmpdir } from "os";
import { join } from "path";
import { platform } from "process";
import { ProviderResult } from "vscode";
import { TypstDebugSession } from "./dap/mockDebug";
import { activateMockDebug, workspaceFileAccessor } from "./dap";

/*
 * The compile time flag 'runMode' controls how the debug adapter is run.
 * Please note: the test suite only supports 'external' mode.
 */
const runMode: "external" | "server" | "namedPipeServer" | "inline" = "inline";

export function activate(context: vscode.ExtensionContext) {
  // debug adapters can be run in different ways by using a vscode.DebugAdapterDescriptorFactory:
  switch (runMode) {
    case "server":
      // run the debug adapter as a server inside the extension and communicate via a socket
      activateMockDebug(context, new MockDebugAdapterServerDescriptorFactory());
      break;

    case "namedPipeServer":
      // run the debug adapter as a server inside the extension and communicate via a named pipe (Windows) or UNIX domain socket (non-Windows)
      activateMockDebug(context, new MockDebugAdapterNamedPipeServerDescriptorFactory());
      break;

    case "external":
    default:
      // run the debug adapter as a separate process
      activateMockDebug(context, new DebugAdapterExecutableFactory());
      break;

    case "inline":
      // run the debug adapter inside the extension and directly talk to it
      activateMockDebug(context);
      break;
  }
}

export function deactivate() {
  // nothing to do
}

class DebugAdapterExecutableFactory implements vscode.DebugAdapterDescriptorFactory {
  // The following use of a DebugAdapter factory shows how to control what debug adapter executable is used.
  // Since the code implements the default behavior, it is absolutely not neccessary and we show it here only for educational purpose.

  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): ProviderResult<vscode.DebugAdapterDescriptor> {
    // param "executable" contains the executable optionally specified in the package.json (if any)

    // use the executable specified in the package.json if it exists or determine it based on some other information (e.g. the session)
    if (!executable) {
      const command = "absolute path to my DA executable";
      const args = ["some args", "another arg"];
      const options = {
        cwd: "working directory for executable",
        env: { envVariable: "some value" },
      };
      executable = new vscode.DebugAdapterExecutable(command, args, options);
    }

    const command = executable.command;
    const rootCwd = executable.options?.cwd || session.workspaceFolder?.uri.fsPath;
    const args = [
      "compile",
      "--debug",
      ...(rootCwd ? ["--root", rootCwd] : []),
      `C:\\Users\\yx\\work\\rust\\typst\\addons\\vscode\\sampleWorkspace\\test2.typ`,
    ];
    executable = new vscode.DebugAdapterExecutable(command, args, {
      ...(executable.options || {}),
      cwd: rootCwd,
    });

    // make VS Code launch the DA executable
    return executable;
  }
}

class MockDebugAdapterServerDescriptorFactory implements vscode.DebugAdapterDescriptorFactory {
  private server?: Net.Server;

  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    if (!this.server) {
      // start listening on a random port
      this.server = Net.createServer((socket) => {
        const session = new TypstDebugSession(workspaceFileAccessor);
        session.setRunAsServer(true);
        session.start(socket as NodeJS.ReadableStream, socket);
      }).listen(0);
    }

    // make VS Code connect to debug server
    return new vscode.DebugAdapterServer((this.server.address() as Net.AddressInfo).port);
  }

  dispose() {
    if (this.server) {
      this.server.close();
    }
  }
}

class MockDebugAdapterNamedPipeServerDescriptorFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  private server?: Net.Server;

  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
    executable: vscode.DebugAdapterExecutable | undefined,
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    if (!this.server) {
      // start listening on a random named pipe path
      const pipeName = randomBytes(10).toString("utf8");
      const pipePath =
        platform === "win32" ? join("\\\\.\\pipe\\", pipeName) : join(tmpdir(), pipeName);

      this.server = Net.createServer((socket) => {
        const session = new TypstDebugSession(workspaceFileAccessor);
        session.setRunAsServer(true);
        session.start(<NodeJS.ReadableStream>socket, socket);
      }).listen(pipePath);
    }

    // make VS Code connect to debug server
    return new vscode.DebugAdapterNamedPipeServer(this.server.address() as string);
  }

  dispose() {
    if (this.server) {
      this.server.close();
    }
  }
}
