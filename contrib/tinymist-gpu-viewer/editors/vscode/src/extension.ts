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
const MIN_VIEWER_INNER_WIDTH = 800;
const MIN_VIEWER_INNER_HEIGHT = 844;

type WindowLayoutMode = "disabled" | "sideBySide";
type PreviewTarget = "paged" | "html";

interface TinymistPreviewer {
  compatibleTinymistVersion: string;
  supportedTargets?: PreviewTarget[];
  isCompatible?(tinymistVersion: string): Promise<boolean> | boolean;
  handlePreview(task: TinymistPreviewTask): Promise<vscode.Disposable> | vscode.Disposable;
}

interface TinymistPreviewTask {
  taskId: string;
  documentPath: string;
  target: PreviewTarget;
  dataPlaneHost: string;
  initialWindowState?: ViewerWindowState;
}

interface TinymistPreviewerProvider {
  providePreviewer(): Promise<TinymistPreviewer> | TinymistPreviewer;
}

interface ViewerWindowState {
  inner_width: number;
  inner_height: number;
  outer_x?: number;
  outer_y?: number;
}

interface WindowRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

interface WindowLaunchPlan {
  initialWindowState?: ViewerWindowState;
  repairSideBySideLayout: boolean;
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
        supportedTargets: ["paged"],
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

async function launchViewer(
  context: vscode.ExtensionContext,
  task: TinymistPreviewTask,
): Promise<vscode.Disposable> {
  activeViewers.get(task.taskId)?.kill();

  const executable = resolveViewerExecutable(context);
  const documentTitle = documentTitleForPath(task.documentPath);
  const viewerTitle = viewerWindowTitle(documentTitle);
  const layoutMode = getWindowLayoutMode();
  const args = ["--data-plane-host", task.dataPlaneHost, "--document-title", documentTitle];
  const windowPlan = await windowLaunchPlanForTask(task, layoutMode);
  appendInitialWindowArgs(args, windowPlan.initialWindowState);
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
  scheduleWindowLayout(viewer, viewerTitle, windowPlan.repairSideBySideLayout);
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
  viewer.on("close", (code, signal) => {
    deleteActiveViewer(task.taskId, viewer);
    appendLog(`Tinymist GPU Viewer exited with code ${code ?? "null"} signal ${signal ?? "null"}`);
  });

  return {
    dispose() {
      deleteActiveViewer(task.taskId, viewer);
      if (!viewer.killed) {
        viewer.kill();
      }
    },
  };
}

function deleteActiveViewer(taskId: string, viewer: ChildProcessWithoutNullStreams) {
  if (activeViewers.get(taskId) === viewer) {
    activeViewers.delete(taskId);
  }
}

async function windowLaunchPlanForTask(
  task: TinymistPreviewTask,
  layoutMode: WindowLayoutMode,
): Promise<WindowLaunchPlan> {
  const storedWindowState = normalizeStoredWindowState(task.initialWindowState);
  if (layoutMode === "sideBySide") {
    const sideBySideState = await prepareSideBySideInitialWindowState();
    if (storedWindowState) {
      appendLog(
        "Using stored viewer window state after side-by-side pre-layout; skipping viewer layout repair.",
      );
      return {
        initialWindowState: storedWindowState,
        repairSideBySideLayout: false,
      };
    }

    return {
      initialWindowState: sideBySideState ?? storedWindowState,
      repairSideBySideLayout: true,
    };
  }

  return {
    initialWindowState: storedWindowState,
    repairSideBySideLayout: false,
  };
}

async function prepareSideBySideInitialWindowState(): Promise<ViewerWindowState | undefined> {
  try {
    const rect = await prepareSideBySideLayoutBeforeSpawn();
    if (!rect) {
      return undefined;
    }

    appendLog(
      `Prepared side-by-side viewer initial window rect ${rect.width}x${rect.height}+${rect.x}+${rect.y}.`,
    );
    return windowStateFromRect(rect);
  } catch (error) {
    appendLog(`Could not prepare side-by-side window layout before launch: ${errorMessage(error)}`);
    return undefined;
  }
}

function appendInitialWindowArgs(args: string[], state: ViewerWindowState | undefined) {
  if (!state) {
    return;
  }

  args.push("--initial-window-inner-size", `${state.inner_width}x${state.inner_height}`);
  if (state.outer_x !== undefined && state.outer_y !== undefined) {
    args.push(`--initial-window-position=${state.outer_x},${state.outer_y}`);
  }
}

function windowStateFromRect(rect: WindowRect): ViewerWindowState | undefined {
  if (!isFiniteInteger(rect.width) || !isFiniteInteger(rect.height)) {
    return undefined;
  }
  if (rect.width <= 0 || rect.height <= 0) {
    return undefined;
  }

  const state: ViewerWindowState = {
    inner_width: rect.width,
    inner_height: rect.height,
  };
  if (isFiniteInteger(rect.x) && isFiniteInteger(rect.y)) {
    state.outer_x = rect.x;
    state.outer_y = rect.y;
  }
  return state;
}

function normalizeStoredWindowState(value: unknown): ViewerWindowState | undefined {
  if (!isObject(value)) {
    return undefined;
  }

  const { inner_width, inner_height, outer_x, outer_y } = value;
  if (
    !isFiniteInteger(inner_width) ||
    !isFiniteInteger(inner_height) ||
    inner_width < MIN_VIEWER_INNER_WIDTH ||
    inner_height < MIN_VIEWER_INNER_HEIGHT
  ) {
    return undefined;
  }

  const state: ViewerWindowState = { inner_width, inner_height };
  if (outer_x !== undefined || outer_y !== undefined) {
    if (!isFiniteInteger(outer_x) || !isFiniteInteger(outer_y)) {
      return undefined;
    }
    state.outer_x = outer_x;
    state.outer_y = outer_y;
  }
  return state;
}

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isFiniteInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value);
}

function resolveViewerExecutable(context: vscode.ExtensionContext): string {
  const configured = vscode.workspace
    .getConfiguration("tinymist.gpuViewer")
    .get<string | null>("executable");
  const configuredPath = configured?.trim();
  if (configuredPath) {
    if (configuredPath === VIEWER_BINARY_NAME || fs.existsSync(configuredPath)) {
      return configuredPath;
    }

    throw new Error(
      `Configured tinymist.gpuViewer.executable does not exist: ${configuredPath}. Unset the setting to use the bundled viewer or ${VIEWER_BINARY_NAME} from PATH.`,
    );
  }

  const candidates = [
    path.join(context.extensionUri.fsPath, "bin", VIEWER_BINARY_NAME),
    VIEWER_BINARY_NAME,
  ];

  for (const candidate of candidates) {
    if (candidate === VIEWER_BINARY_NAME || fs.existsSync(candidate)) {
      return candidate;
    }
  }

  throw new Error(
    `Cannot find ${VIEWER_BINARY_NAME}. Configure tinymist.gpuViewer.executable, bundle it under bin, or add it to PATH.`,
  );
}

function documentTitleForPath(documentPath: string): string {
  const title = path.basename(documentPath).trim();
  return title || documentPath.trim() || VIEWER_WINDOW_TITLE;
}

function viewerWindowTitle(documentTitle: string): string {
  const title = documentTitle.trim();
  return title ? `${title} - ${VIEWER_WINDOW_TITLE}` : VIEWER_WINDOW_TITLE;
}

function scheduleWindowLayout(
  viewer: ChildProcessWithoutNullStreams,
  viewerTitle: string,
  repairSideBySideLayout: boolean,
) {
  if (!repairSideBySideLayout) {
    appendLog("Skipping side-by-side window layout repair.");
    return;
  }

  const viewerPid = viewer.pid;
  if (viewerPid === undefined) {
    appendLog("Skipping side-by-side window layout: viewer process id is unavailable.");
    return;
  }

  appendLog(`Scheduling side-by-side window layout repair for viewer pid ${viewerPid}.`);
  void arrangeWindowsSideBySide(viewerPid, viewerTitle).catch((error) => {
    appendLog(`Could not arrange GPU viewer windows: ${errorMessage(error)}`);
  });
}

function getWindowLayoutMode(): WindowLayoutMode {
  const configured = vscode.workspace
    .getConfiguration("tinymist.gpuViewer")
    .get<string>("windowLayout", "sideBySide");

  return configured === "sideBySide" ? "sideBySide" : "disabled";
}

async function prepareSideBySideLayoutBeforeSpawn(): Promise<WindowRect | undefined> {
  switch (process.platform) {
    case "win32":
      return prepareSideBySideLayoutBeforeSpawnWin32();
    case "darwin":
      return prepareSideBySideLayoutBeforeSpawnMacOS();
    case "linux":
      return prepareSideBySideLayoutBeforeSpawnLinux();
    default:
      appendLog(`Skipping side-by-side pre-layout: unsupported platform ${process.platform}.`);
      return undefined;
  }
}

async function prepareSideBySideLayoutBeforeSpawnWin32(): Promise<WindowRect> {
  const script = `
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

$codeWindow = Get-VisibleTopLevelWindows | Where-Object { Test-CodeWindow $_ } | Select-Object -First 1
if (-not $codeWindow) {
    throw 'Could not find a visible VS Code window.'
}

$workArea = [System.Windows.Forms.Screen]::PrimaryScreen.WorkingArea
$halfWidth = [Math]::Floor($workArea.Width / 2)
$rightWidth = $workArea.Width - $halfWidth

[void][TinymistWindowApi]::ShowWindowAsync($codeWindow.Handle, 9)
[void][TinymistWindowApi]::MoveWindow($codeWindow.Handle, $workArea.Left, $workArea.Top, $halfWidth, $workArea.Height, $true)

[pscustomobject]@{
    x = [int] ($workArea.Left + $halfWidth)
    y = [int] $workArea.Top
    width = [int] $rightWidth
    height = [int] $workArea.Height
} | ConvertTo-Json -Compress
`;

  const { stdout } = await runFile("powershell.exe", [
    "-NoProfile",
    "-ExecutionPolicy",
    "Bypass",
    "-Command",
    script,
  ]);
  const rect = parseWindowRectJson(stdout);
  if (!rect) {
    throw new Error("Could not parse viewer window rect from PowerShell output.");
  }
  return rect;
}

async function prepareSideBySideLayoutBeforeSpawnMacOS(): Promise<WindowRect> {
  const script = `
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
set viewerLeft to screenLeft + halfWidth

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
  if not (exists window 1 of codeProcess) then
    error "Could not find a VS Code window."
  end if

  set position of window 1 of codeProcess to {screenLeft, screenTop}
  set size of window 1 of codeProcess to {halfWidth, screenHeight}
end tell

return (viewerLeft as string) & "," & (screenTop as string) & "," & (rightWidth as string) & "," & (screenHeight as string)
`;

  const { stdout } = await runFile("osascript", ["-e", script]);
  const rect = parseWindowRectCsv(stdout);
  if (!rect) {
    throw new Error("Could not parse viewer window rect from AppleScript output.");
  }
  return rect;
}

async function prepareSideBySideLayoutBeforeSpawnLinux(): Promise<WindowRect> {
  const windows = await getLinuxWindows();
  const code = windows.find(isLinuxCodeWindow);
  if (!code) {
    throw new Error("Could not find a VS Code window.");
  }

  const rects = splitSideBySideWorkArea(await getLinuxWorkArea());
  await moveLinuxWindow(
    code.id,
    rects.code.x,
    rects.code.y,
    rects.code.width,
    rects.code.height,
  );
  return rects.viewer;
}

async function arrangeWindowsSideBySide(viewerPid: number, viewerTitle: string) {
  await delay(WINDOW_LAYOUT_DELAY_MS);

  switch (process.platform) {
    case "win32":
      await arrangeWindowsSideBySideWin32(viewerPid, viewerTitle);
      return;
    case "darwin":
      await arrangeWindowsSideBySideMacOS(viewerTitle);
      return;
    case "linux":
      await arrangeWindowsSideBySideLinux(viewerPid);
      return;
    default:
      appendLog(`Skipping side-by-side window layout: unsupported platform ${process.platform}.`);
  }
}

async function arrangeWindowsSideBySideWin32(viewerPid: number, viewerTitle: string) {
  const viewerTitleLiteral = powershellSingleQuotedString(viewerTitle);
  const script = `
$viewerPid = ${viewerPid}
$viewerTitle = ${viewerTitleLiteral}
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

async function arrangeWindowsSideBySideMacOS(viewerTitle: string) {
  const viewerTitleLiteral = appleScriptStringLiteral(viewerTitle);
  const script = `
set viewerTitle to ${viewerTitleLiteral}
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
        if name of candidateWindow contains viewerTitle then
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

function powershellSingleQuotedString(value: string): string {
  return `'${value.replace(/'/g, "''")}'`;
}

function appleScriptStringLiteral(value: string): string {
  return `"${value.replace(/\\/g, "\\\\").replace(/"/g, '\\"')}"`;
}

async function arrangeWindowsSideBySideLinux(viewerPid: number) {
  const pair = await waitForWindowPair(viewerPid);
  const rects = splitSideBySideWorkArea(await getLinuxWorkArea());

  await moveLinuxWindow(
    pair.code.id,
    rects.code.x,
    rects.code.y,
    rects.code.width,
    rects.code.height,
  );
  await moveLinuxWindow(
    pair.viewer.id,
    rects.viewer.x,
    rects.viewer.y,
    rects.viewer.width,
    rects.viewer.height,
  );
}

function splitSideBySideWorkArea(workArea: WindowRect): { code: WindowRect; viewer: WindowRect } {
  const halfWidth = Math.floor(workArea.width / 2);
  const rightWidth = workArea.width - halfWidth;
  return {
    code: {
      x: workArea.x,
      y: workArea.y,
      width: halfWidth,
      height: workArea.height,
    },
    viewer: {
      x: workArea.x + halfWidth,
      y: workArea.y,
      width: rightWidth,
      height: workArea.height,
    },
  };
}

function parseWindowRectJson(stdout: string): WindowRect | undefined {
  const lines = stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
  const line = lines[lines.length - 1];
  if (!line) {
    return undefined;
  }

  try {
    return normalizeWindowRect(JSON.parse(line));
  } catch {
    return undefined;
  }
}

function parseWindowRectCsv(stdout: string): WindowRect | undefined {
  const values = stdout
    .trim()
    .split(",")
    .map((value) => Number(value.trim()));
  if (values.length !== 4) {
    return undefined;
  }

  return normalizeWindowRect({
    x: values[0],
    y: values[1],
    width: values[2],
    height: values[3],
  });
}

function normalizeWindowRect(value: unknown): WindowRect | undefined {
  if (!isObject(value)) {
    return undefined;
  }

  const { x, y, width, height } = value;
  if (
    !isFiniteInteger(x) ||
    !isFiniteInteger(y) ||
    !isFiniteInteger(width) ||
    !isFiniteInteger(height) ||
    width <= 0 ||
    height <= 0
  ) {
    return undefined;
  }

  return { x, y, width, height };
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

async function getLinuxWorkArea(): Promise<WindowRect> {
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
