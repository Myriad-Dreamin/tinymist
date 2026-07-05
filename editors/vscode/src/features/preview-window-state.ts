import * as vscode from "vscode";

export const VIEWER_WINDOW_STATE_SCHEMA_VERSION = 1;
const VIEWER_WINDOW_STATE_STORAGE_KEY = "tinymist.gpuViewer.windowState.v1";
const MIN_VIEWER_INNER_WIDTH = 800;
const MIN_VIEWER_INNER_HEIGHT = 844;

export interface ViewerWindowState {
  inner_width: number;
  inner_height: number;
  outer_x?: number;
  outer_y?: number;
}

interface ViewerWindowStateRecord {
  schema_version: number;
  window: ViewerWindowState;
}

interface ViewerWindowStateNotification {
  task_id?: string;
  schema_version: number;
  window: ViewerWindowState;
}

let viewerWindowStateWriteQueue: Promise<void> = Promise.resolve();

export function loadStoredViewerWindowState(
  context: vscode.ExtensionContext,
): ViewerWindowState | undefined {
  const record = context.globalState.get<ViewerWindowStateRecord>(VIEWER_WINDOW_STATE_STORAGE_KEY);
  if (!isObject(record) || record.schema_version !== VIEWER_WINDOW_STATE_SCHEMA_VERSION) {
    return undefined;
  }

  return normalizeViewerWindowState(record.window);
}

export async function saveStoredViewerWindowState(
  context: vscode.ExtensionContext,
  notification: unknown,
) {
  const payload = normalizeViewerWindowStateNotification(notification);
  if (!payload) {
    return;
  }

  viewerWindowStateWriteQueue = viewerWindowStateWriteQueue
    .catch(() => undefined)
    .then(async () => {
      const window = mergeViewerWindowState(loadStoredViewerWindowState(context), payload.window);
      await context.globalState.update(VIEWER_WINDOW_STATE_STORAGE_KEY, {
        schema_version: VIEWER_WINDOW_STATE_SCHEMA_VERSION,
        window,
      } satisfies ViewerWindowStateRecord);
    });
  await viewerWindowStateWriteQueue;
}

function normalizeViewerWindowStateNotification(
  value: unknown,
): ViewerWindowStateNotification | undefined {
  if (!isObject(value) || value.schema_version !== VIEWER_WINDOW_STATE_SCHEMA_VERSION) {
    return undefined;
  }

  const window = normalizeViewerWindowState(value.window);
  if (!window) {
    return undefined;
  }

  return {
    task_id: typeof value.task_id === "string" ? value.task_id : undefined,
    schema_version: VIEWER_WINDOW_STATE_SCHEMA_VERSION,
    window,
  };
}

function mergeViewerWindowState(
  previous: ViewerWindowState | undefined,
  next: ViewerWindowState,
): ViewerWindowState {
  if (next.outer_x !== undefined && next.outer_y !== undefined) {
    return next;
  }

  if (previous?.outer_x === undefined || previous.outer_y === undefined) {
    return next;
  }

  return {
    ...next,
    outer_x: previous.outer_x,
    outer_y: previous.outer_y,
  };
}

function normalizeViewerWindowState(value: unknown): ViewerWindowState | undefined {
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
