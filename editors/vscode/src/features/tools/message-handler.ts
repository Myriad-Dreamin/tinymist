import type { EditorToolContext } from ".";
import { messageHandlers } from "./message-handlers";

export interface WebviewMessage {
  type: string;
}

export type MessageHandler = (
  // biome-ignore lint/suspicious/noExplicitAny: type-erased
  message: any,
  context: EditorToolContext,
) => Promise<void> | void;

export async function handleMessage(
  message: WebviewMessage,
  context: EditorToolContext,
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
