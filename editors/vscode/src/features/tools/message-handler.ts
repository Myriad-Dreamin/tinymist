import type * as vscode from "vscode";
import type { ExtensionContext } from "../../state";
import { messageHandlers } from "./message-handlers";

export interface MessageHandlerContext {
  context: ExtensionContext;
  panel: vscode.WebviewView | vscode.WebviewPanel;
  dispose: () => void;
  addDisposable: (disposable: vscode.Disposable) => void;
}

export interface WebviewMessage {
  type: string;
}

export type MessageHandler = (
  // biome-ignore lint/suspicious/noExplicitAny: type-erased
  message: any,
  context: MessageHandlerContext,
) => Promise<void> | void;

export async function handleMessage(
  message: WebviewMessage,
  context: MessageHandlerContext,
): Promise<boolean> {
  const handler = messageHandlers[message.type];
  if (handler) {
    try {
      await handler(message, context);
      return true;
    } catch (error) {
      console.error(`Error handling message ${message}:`, error);
      return false;
    }
  }
  return false;
}
