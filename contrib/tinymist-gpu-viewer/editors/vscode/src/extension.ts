import * as vscode from "vscode";
import * as fs from "fs";
import * as path from "path";
import {
  execFile,
  spawn,
  type ChildProcessWithoutNullStreams,
  type ExecFileOptionsWithStringEncoding,
} from "child_process";

const VIEWER_BINARY_NAME = process.platform === "win32" ? "tinymist-viewer.exe" : "tinymist-viewer";
const VIEWER_WINDOW_TITLE = "Tinymist View";
const WINDOW_LAYOUT_DELAY_MS = 700;
const WINDOW_LAYOUT_TIMEOUT_MS = 10_000;
const WINDOW_LAYOUT_POLL_MS = 250;

type WindowLayoutMode = "disabled" | "sideBySide";

interface TinymistPreviewer {
  compatibleTinymistVersion: string;
  isCompatible?(tinymistVersion: string): Promise<boolean> | boolean;
  handlePreview(task: TinymistPreviewTask): Promise<vscode.Disposable> | vscode.Disposable;
}

interface TinymistPreviewTask {
  taskId: string;
  documentPath: string;
  dataPlaneHost: string;
}

interface TinymistPreviewerProvider {
  providePreviewer(): Promise<TinymistPreviewer> | TinymistPreviewer;
}

let outputChannel: vscode.OutputChannel | undefined;
const activeViewers = new Map<string, ChildProcessWithoutNullStreams>();

export function activate(context: vscode.ExtensionContext): TinymistPreviewerProvider {
  const compatibleTinymistVersion = String(context.extension.packageJSON.version ?? "0.0.0");
  outputChannel = vscode.window.createOutputChannel("Tinymist GPU Viewer");
  context.subscriptions.push(outputChannel, {
    dispose() {
      for (const viewer of activeViewers.values()) {
        viewer.kill();
      }
      activeViewers.clear();
    },
  });

  return {
    providePreviewer() {
      return {
        compatibleTinymistVersion,
        isCompatible(tinymistVersion: string) {
          return tinymistVersion === compatibleTinymistVersion;
        },
        handlePreview(task: TinymistPreviewTask) {
          return launchViewer(context, task);
        },
      };
    },
  };
}

export function deactivate() {}

function launchViewer(context: vscode.ExtensionContext, task: TinymistPreviewTask): vscode.Disposable {
  activeViewers.get(task.taskId)?.kill();

  const executable = resolveViewerExecutable(context);
  const args = ["--data-plane-host", task.dataPlaneHost];
  const cwd = path.dirname(task.documentPath);
  appendLog(`Starting ${executable} ${args.join(" ")}`);

  const viewer = spawn(executable, args, {
    cwd,
    env: {
      ...process.env,
      RUST_BACKTRACE: "1",
    },
    windowsHide: false,
  });

  activeViewers.set(task.taskId, viewer);
  scheduleWindowLayout(viewer);
  viewer.stdout.on("data", (data: Buffer) => appendLog(data.toString()));
  viewer.stderr.on("data", (data: Buffer) => appendLog(data.toString()));
  viewer.on("error", (error) => {
    const message = `Failed to start Tinymist GPU Viewer: ${error.message}`;
    appendLog(message);
    void vscode.window.showErrorMessage(message, "Show Logs").then((selection) => {
      if (selection === "Show Logs") {
        outputChannel?.show();
      }
    });
  });
  viewer.on("exit", (code, signal) => {
    activeViewers.delete(task.taskId);
    appendLog(`Tinymist GPU Viewer exited with code ${code ?? "null"} signal ${signal ?? "null"}`);
  });

  return {
    dispose() {
      activeViewers.delete(task.taskId);
      if (!viewer.killed) {
        viewer.kill();
      }
    },
  };
}

function resolveViewerExecutable(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace
    .getConfiguration("tinymist.gpuViewer")
    .get<string | null>("executable");
  const candidates = [
    configured,
    path.join(context.extensionUri.fsPath, "bin", VIEWER_BINARY_NAME),
    VIEWER_BINARY_NAME,
  ].filter((candidate): candidate is string => !!candidate && candidate.trim() !== "");

  for (const candidate of candidates) {
    if (candidate === VIEWER_BINARY_NAME || fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `Cannot find ${VIEWER_BINARY_NAME}. Configure tinymist.gpuViewer.executable, bundle it under bin, or add it to PATH.`,
  );
}

function scheduleWindowLayout(viewer: ChildProcessWithoutNullStreams) {
  if (getWindowLayoutMode() !== "sideBySide") {
    appendLog("Skipping side-by-side window layout: tinymist.gpuViewer.windowLayout is disabled.");
    return;
  }

  const viewerPid = viewer.pid;
  if (viewerPid === undefined) {
    appendLog("Skipping side-by-side window layout: viewer process id is unavailable.");
    return;
  }

  appendLog(`Scheduling side-by-side window layout for viewer pid ${viewerPid}.`);
  void arrangeWindowsSideBySide(viewerPid).catch((error) => {
    appendLog(`Could not arrange GPU viewer windows: ${errorMessage(error)}`);
  });
}

function getWindowLayoutMode(): WindowLayoutMode {
  const configured = vscode.workspace
    .getConfiguration("tinymist.gpuViewer")
    .get<string>("windowLayout", "sideBySide");

  return configured === "sideBySide" ? "sideBySide" : "disabled";
}

async function arrangeWindowsSideBySide(viewerPid: number) {
  await delay(WINDOW_LAYOUT_DELAY_MS);

  switch (process.platform) {
    case "win32":
      await arrangeWindowsSideBySideWin32(viewerPid);
      return;
    case "darwin":
      await arrangeWindowsSideBySideMacOS();
      return;
    case "linux":
      await arrangeWindowsSideBySideLinux(viewerPid);
      return;
    default:
      appendLog(`Skipping side-by-side window layout: unsupported platform ${process.platform}.`);
  }
}

async function arrangeWindowsSideBySideWin32(viewerPid: number) {
  const script = `
$viewerPid = ${viewerPid}
$viewerTitle = '${VIEWER_WINDOW_TITLE}'
$codeProcessNames = @('Code', 'Code - Insiders', 'VSCodium', 'Code - OSS')

Add-Type -AssemblyName System.Windows.Forms
Add-Type @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class TinymistWindowApi {
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool EnumWindows(EnumWindowsProc enumProc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowText(IntPtr hWnd, StringBuilder text, int count);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint processId);

    [DllImport("user32.dll")]
    public static extern bool MoveWindow(IntPtr hWnd, int x, int y, int width, int height, bool repaint);

    [DllImport("user32.dll")]
    public static extern bool ShowWindowAsync(IntPtr hWnd, int command);
}
'@

function Get-VisibleTopLevelWindows {
    $script:tinymistWindows = New-Object 'System.Collections.Generic.List[object]'
    $callback = [TinymistWindowApi+EnumWindowsProc]{
        param([IntPtr] $handle, [IntPtr] $param)

        if (-not [TinymistWindowApi]::IsWindowVisible($handle)) {
            return $true
        }

        $titleBuilder = New-Object System.Text.StringBuilder 512
        [void][TinymistWindowApi]::GetWindowText($handle, $titleBuilder, $titleBuilder.Capacity)
        $title = $titleBuilder.ToString()
        if ([string]::IsNullOrWhiteSpace($title)) {
            return $true
        }

        [uint32] $windowPid = 0
        [void][TinymistWindowApi]::GetWindowThreadProcessId($handle, [ref] $windowPid)
        $script:tinymistWindows.Add([pscustomobject]@{
            Handle = $handle
            ProcessId = [int] $windowPid
            Title = $title
        })

        return $true
    }

    [void][TinymistWindowApi]::EnumWindows($callback, [IntPtr]::Zero)
    return $script:tinymistWindows
}

function Test-CodeWindow($window) {
    try {
        $processName = (Get-Process -Id $window.ProcessId -ErrorAction Stop).ProcessName
    } catch {
        $processName = ''
    }

    return (($codeProcessNames -contains $processName) -or ($window.Title -like '*Visual Studio Code*') -or ($window.Title -like '*VSCodium*') -or ($window.Title -like '*Code - OSS*'))
}

$deadline = (Get-Date).AddMilliseconds(${WINDOW_LAYOUT_TIMEOUT_MS})
$codeWindow = $null
$viewerWindow = $null

do {
    $windows = Get-VisibleTopLevelWindows
    $viewerWindow = $windows | Where-Object { $_.ProcessId -eq $viewerPid -or $_.Title -eq $viewerTitle } | Select-Object -First 1
    $codeWindow = $windows | Where-Object { Test-CodeWindow $_ } | Select-Object -First 1

    if ($codeWindow -and $viewerWindow) {
        break
    }

    Start-Sleep -Milliseconds ${WINDOW_LAYOUT_POLL_MS}
} while ((Get-Date) -lt $deadline)

if (-not $codeWindow) {
    throw 'Could not find a visible VS Code window.'
}
if (-not $viewerWindow) {
    throw 'Could not find the Tinymist GPU Viewer window.'
}

$workArea = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$halfWidth = [Math]::Floor($workArea.Width / 2)
$rightWidth = $workArea.Width - $halfWidth

[void][TinymistWindowApi]::ShowWindowAsync($codeWindow.Handle, 9)
[void][TinymistWindowApi]::ShowWindowAsync($viewerWindow.Handle, 9)
[void][TinymistWindowApi]::MoveWindow($codeWindow.Handle, $workArea.Left, $workArea.Top, $halfWidth, $workArea.Height, $true)
[void][TinymistWindowApi]::MoveWindow($viewerWindow.Handle, $workArea.Left + $halfWidth, $workArea.Top, $rightWidth, $workArea.Height, $true)
`;

  await runFile("powershell.exe", ["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script]);
}

async function arrangeWindowsSideBySideMacOS() {
  const script = `
set viewerTitle to "${VIEWER_WINDOW_TITLE}"
set codeProcessNames to {"Code", "Visual Studio Code", "Code - Insiders", "VSCodium", "Code - OSS"}
tell application "Finder"
  set desktopBounds to bounds of window of desktop
end tell

set screenLeft to item 1 of desktopBounds
set screenTop to item 2 of desktopBounds
set screenRight to item 3 of desktopBounds
set screenBottom to item 4 of desktopBounds
set screenWidth to screenRight - screenLeft
set screenHeight to screenBottom - screenTop
set halfWidth to screenWidth div 2
set rightWidth to screenWidth - halfWidth

tell application "System Events"
  set codeProcess to missing value
  repeat with processName in codeProcessNames
    if exists process (contents of processName) then
      set codeProcess to process (contents of processName)
      exit repeat
    end if
  end repeat

  if codeProcess is missing value then
    error "Could not find a VS Code process."
  end if

  set deadline to (current date) + 10
  set viewerWindow to missing value
  repeat while viewerWindow is missing value and (current date) < deadline
    repeat with candidateProcess in processes
      repeat with candidateWindow in windows of candidateProcess
        if name of candidateWindow is viewerTitle then
          set viewerWindow to candidateWindow
          exit repeat
        end if
      end repeat
      if viewerWindow is not missing value then exit repeat
    end repeat

    if viewerWindow is missing value then delay 0.25
  end repeat

  if viewerWindow is missing value then
    error "Could not find the Tinymist GPU Viewer window."
  end if
  if not (exists window 1 of codeProcess) then
    error "Could not find a VS Code window."
  end if

  set position of window 1 of codeProcess to {screenLeft, screenTop}
  set size of window 1 of codeProcess to {halfWidth, screenHeight}
  set position of viewerWindow to {screenLeft + halfWidth, screenTop}
  set size of viewerWindow to {rightWidth, screenHeight}
end tell
`;

  await runFile("osascript", ["-e", script]);
}

async function arrangeWindowsSideBySideLinux(viewerPid: number) {
  const pair = await waitForWindowPair(viewerPid);
  const workArea = await getLinuxWorkArea();
  const halfWidth = Math.floor(workArea.width / 2);
  const rightWidth = workArea.width - halfWidth;

  await moveLinuxWindow(pair.code.id, workArea.x, workArea.y, halfWidth, workArea.height);
  await moveLinuxWindow(
    pair.viewer.id,
    workArea.x + halfWidth,
    workArea.y,
    rightWidth,
    workArea.height,
  );
}

interface CommandResult {
  stdout: string;
  stderr: string;
}

function runFile(
  file: string,
  args: string[],
  timeout = WINDOW_LAYOUT_TIMEOUT_MS,
): Promise<CommandResult> {
  const options: ExecFileOptionsWithStringEncoding = {
    encoding: "utf8",
    timeout,
    windowsHide: true,
  };

  return new Promise((resolve, reject) => {
    execFile(file, args, options, (error, stdout, stderr) => {
      if (error) {
        const details = stderr.trim() ? `${error.message}: ${stderr.trim()}` : error.message;
        reject(new Error(`${file} failed: ${details}`));
        return;
      }

      resolve({ stdout, stderr });
    });
  });
}

interface LinuxWindow {
  id: string;
  pid: number;
  title: string;
}

interface LinuxWorkArea {
  x: number;
  y: number;
  width: number;
  height: number;
}

async function waitForWindowPair(
  viewerPid: number,
): Promise<{ code: LinuxWindow; viewer: LinuxWindow }> {
  const started = Date.now();
  while (Date.now() - started < WINDOW_LAYOUT_TIMEOUT_MS) {
    const windows = await getLinuxWindows();
    const viewer = windows.find(
      (window) => window.pid === viewerPid || window.title.includes(VIEWER_WINDOW_TITLE),
    );
    const code = windows.find(isLinuxCodeWindow);

    if (viewer && code) {
      return { code, viewer };
    }

    await delay(WINDOW_LAYOUT_POLL_MS);
  }

  throw new Error("Could not find both VS Code and Tinymist GPU Viewer windows.");
}

async function getLinuxWindows(): Promise<LinuxWindow[]> {
  const { stdout } = await runFile("wmctrl", ["-l", "-p", "-G"]);
  return stdout
    .split(/\r?\n/)
    .map((line) => line.trimEnd())
    .filter((line) => line.length > 0)
    .flatMap(parseLinuxWindowLine);
}

function parseLinuxWindowLine(line: string): LinuxWindow[] {
  const match = line.match(/^(\S+)\s+\S+\s+(-?\d+)\s+-?\d+\s+-?\d+\s+\d+\s+\d+\s+\S+\s*(.*)$/);
  if (!match) {
    return [];
  }

  return [
    {
      id: match[1],
      pid: Number(match[2]),
      title: match[3] ?? "",
    },
  ];
}

function isLinuxCodeWindow(window: LinuxWindow): boolean {
  return (
    window.title.includes("Visual Studio Code") ||
    window.title.includes("VSCodium") ||
    window.title.includes("Code - OSS") ||
    window.title.endsWith(" - Code")
  );
}

async function getLinuxWorkArea(): Promise<LinuxWorkArea> {
  const { stdout } = await runFile("wmctrl", ["-d"]);
  const desktop =
    stdout.split(/\r?\n/).find((line) => line.includes("*")) ??
    stdout.split(/\r?\n/).find((line) => line.trim());
  if (!desktop) {
    throw new Error("Could not read desktop geometry from wmctrl.");
  }

  const workAreaMatch = desktop.match(/WA:\s*(-?\d+),(-?\d+)\s+(\d+)x(\d+)/);
  if (workAreaMatch) {
    return {
      x: Number(workAreaMatch[1]),
      y: Number(workAreaMatch[2]),
      width: Number(workAreaMatch[3]),
      height: Number(workAreaMatch[4]),
    };
  }

  const desktopGeometryMatch = desktop.match(/DG:\s*(\d+)x(\d+)/);
  if (desktopGeometryMatch) {
    return {
      x: 0,
      y: 0,
      width: Number(desktopGeometryMatch[1]),
      height: Number(desktopGeometryMatch[2]),
    };
  }

  throw new Error("Could not parse desktop work area from wmctrl.");
}

async function moveLinuxWindow(
  windowId: string,
  x: number,
  y: number,
  width: number,
  height: number,
) {
  await runFile("wmctrl", ["-ir", windowId, "-b", "remove,maximized_vert,maximized_horz"]);
  await runFile("wmctrl", ["-ir", windowId, "-e", `0,${x},${y},${width},${height}`]);
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function appendLog(message: string) {
  outputChannel?.append(message.endsWith("\n") ? message : `${message}\n`);
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}
