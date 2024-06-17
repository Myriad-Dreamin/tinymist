export class TypstCancellationToken {
  isCancellationRequested: boolean = false;
  private _onCancelled: Promise<void>;
  private _onCancelledResolveResolved: Promise<() => void>;

  constructor() {
    let resolveT: () => void = undefined!;
    let resolveX: (_: () => void) => void = undefined!;
    this._onCancelled = new Promise((resolve) => {
      resolveT = resolve;
      if (resolveX) {
        resolveX(resolve);
      }
    });
    this._onCancelledResolveResolved = new Promise((resolve) => {
      resolveX = resolve;
      if (resolveT) {
        resolve(resolveT);
      }
    });
  }

  async cancel(): Promise<void> {
    await this._onCancelledResolveResolved;
    this.isCancellationRequested = true;
  }

  isCancelRequested(): boolean {
    return this.isCancellationRequested;
  }

  async consume(): Promise<void> {
    (await this._onCancelledResolveResolved)();
  }

  wait(): Promise<void> {
    return this._onCancelled;
  }
}
