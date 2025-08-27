import type * as vscode from "vscode";

/**
 * Simple disposal manager for cleaning up resources
 */
export class DisposalManager {
  private disposables: vscode.Disposable[] = [];
  private disposed = false;

  /**
   * Add a disposable resource
   */
  add(disposable: vscode.Disposable): void {
    if (this.disposed) {
      disposable.dispose();
    } else {
      this.disposables.push(disposable);
    }
  }

  /**
   * Dispose all resources
   */
  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;

    for (const disposable of this.disposables) {
      disposable.dispose();
    }
    this.disposables.length = 0;
  }

  /**
   * Check if disposed
   */
  get isDisposed(): boolean {
    return this.disposed;
  }
}
