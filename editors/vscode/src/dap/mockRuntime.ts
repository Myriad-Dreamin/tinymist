/*---------------------------------------------------------
 * Copyright (C) Microsoft Corporation. All rights reserved.
 *--------------------------------------------------------*/

import { EventEmitter } from "events";

export interface FileAccessor {
  isWindows: boolean;
  readFile(path: string): Promise<Uint8Array>;
  writeFile(path: string, contents: Uint8Array): Promise<void>;
}

export interface IRuntimeBreakpoint {
  id: number;
  line: number;
  verified: boolean;
}

interface IRuntimeStepInTargets {
  id: number;
  label: string;
}

interface IRuntimeStackFrame {
  index: number;
  name: string;
  file: string;
  line: number;
  column?: number;
  instruction?: number;
}

interface IRuntimeStack {
  count: number;
  frames: IRuntimeStackFrame[];
}

interface RuntimeDisassembledInstruction {
  address: number;
  instruction: string;
  line?: number;
}

export type IRuntimeVariableType = number | boolean | string | RuntimeVariable[];

export class RuntimeVariable {
  private _memory?: Uint8Array;

  public reference?: number;

  public get value() {
    return this._value;
  }

  public set value(value: IRuntimeVariableType) {
    this._value = value;
    this._memory = undefined;
  }

  public get memory() {
    if (this._memory === undefined && typeof this._value === "string") {
      this._memory = new TextEncoder().encode(this._value);
    }
    return this._memory;
  }

  constructor(
    public readonly name: string,
    private _value: IRuntimeVariableType,
  ) {}

  public setMemory(data: Uint8Array, offset = 0) {
    const memory = this.memory;
    if (!memory) {
      return;
    }

    memory.set(data, offset);
    this._memory = memory;
    this._value = new TextDecoder().decode(memory);
  }
}

interface Word {
  name: string;
  line: number;
  index: number;
}

export function timeout(ms: number) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/**
 * A Mock runtime with minimal debugger functionality.
 * MockRuntime is a hypothetical (aka "Mock") "execution engine with debugging support":
 * it takes a Markdown (*.md) file and "executes" it by "running" through the text lines
 * and searching for "command" patterns that trigger some debugger related functionality (e.g. exceptions).
 * When it finds a command it typically emits an event.
 * The runtime can not only run through the whole file but also executes one line at a time
 * and stops on lines for which a breakpoint has been registered. This functionality is the
 * core of the "debugging support".
 * Since the MockRuntime is completely independent from VS Code or the Debug Adapter Protocol,
 * it can be viewed as a simplified representation of a real "execution engine" (e.g. node.js)
 * or debugger (e.g. gdb).
 * When implementing your own debugger extension for VS Code, you probably don't need this
 * class because you can rely on some existing debugger or runtime.
 */
export class MockRuntime extends EventEmitter {
  // the initial (and one and only) file we are 'debugging'
  private _sourceFile: string = "";
  public get sourceFile() {
    return this._sourceFile;
  }

  private variables = new Map<string, RuntimeVariable>();

  // the contents (= lines) of the one and only file
  private sourceLines: string[] = [];
  private instructions: Word[] = [];
  private starts: number[] = [];
  private ends: number[] = [];

  // This is the next line that will be 'executed'
  private _currentLine = 0;
  private get currentLine() {
    return this._currentLine;
  }
  private set currentLine(x) {
    this._currentLine = x;
    this.instruction = this.starts[x];
  }
  private currentColumn: number | undefined;

  // This is the next instruction that will be 'executed'
  public instruction = 0;

  // maps from sourceFile to array of IRuntimeBreakpoint
  private breakPoints = new Map<string, IRuntimeBreakpoint[]>();

  // all instruction breakpoint addresses
  private instructionBreakpoints = new Set<number>();

  // since we want to send breakpoint events, we will assign an id to every event
  // so that the frontend can match events with breakpoints.
  private breakpointId = 1;

  private breakAddresses = new Map<string, string>();

  private namedException: string | undefined;
  private otherExceptions = false;

  constructor(private fileAccessor: FileAccessor) {
    super();
  }

  /**
   * Start executing the given program.
   */
  public async start(program: string, stopOnEntry: boolean, debug: boolean): Promise<void> {
    await this.loadSource(program);

    if (debug) {
      await this.verifyBreakpoints(this._sourceFile);

      if (stopOnEntry) {
        this.findNextStatement(false, "stopOnEntry");
      } else {
        // we just start to run until we hit a breakpoint, an exception, or the end of the program
        this.continue(false);
      }
    } else {
      this.continue(false);
    }
  }

  /**
   * Continue execution to the end/beginning.
   */
  public continue(reverse: boolean) {
    while (!this.executeLine(this.currentLine, reverse)) {
      if (this.updateCurrentLine(reverse)) {
        break;
      }
      if (this.findNextStatement(reverse)) {
        break;
      }
    }
  }

  /**
   * Step to the next/previous non empty line.
   */
  public step(instruction: boolean, reverse: boolean) {
    if (instruction) {
      if (reverse) {
        this.instruction--;
      } else {
        this.instruction++;
      }
      this.sendEvent("stopOnStep");
    } else {
      this.currentLine++;
      this.currentColumn = this.currentColumn || 0;
      this.sendEvent("stopOnStep");
      //   if (!this.executeLine(this.currentLine, reverse)) {
      //     if (!this.updateCurrentLine(reverse)) {
      //       this.findNextStatement(reverse, "stopOnStep");
      //     }
      //   }
    }
  }

  private updateCurrentLine(reverse: boolean): boolean {
    if (reverse) {
      if (this.currentLine > 0) {
        this.currentLine--;
      } else {
        // no more lines: stop at first line
        this.currentLine = 0;
        this.currentColumn = undefined;
        this.sendEvent("stopOnEntry");
        return true;
      }
    } else {
      if (this.currentLine < this.sourceLines.length - 1) {
        this.currentLine++;
      } else {
        // no more lines: run to end
        this.currentColumn = undefined;
        this.sendEvent("end");
        return true;
      }
    }
    return false;
  }

  /**
   * "Step into" for Mock debug means: go to next character
   */
  public stepIn(targetId: number | undefined) {
    if (typeof targetId === "number") {
      this.currentColumn = targetId;
      this.sendEvent("stopOnStep");
    } else {
      if (typeof this.currentColumn === "number") {
        if (this.currentColumn <= this.sourceLines[this.currentLine].length) {
          this.currentColumn += 1;
        }
      } else {
        this.currentColumn = 1;
      }
      this.sendEvent("stopOnStep");
    }
  }

  /**
   * "Step out" for Mock debug means: go to previous character
   */
  public stepOut() {
    if (typeof this.currentColumn === "number") {
      this.currentColumn -= 1;
      if (this.currentColumn === 0) {
        this.currentColumn = undefined;
      }
    }
    this.sendEvent("stopOnStep");
  }

  public getStepInTargets(frameId: number): IRuntimeStepInTargets[] {
    const line = this.getLine();
    const words = this.getWords(this.currentLine, line);

    // return nothing if frameId is out of range
    if (frameId < 0 || frameId >= words.length) {
      return [];
    }

    const { name, index } = words[frameId];

    // make every character of the frame a potential "step in" target
    return name.split("").map((c, ix) => {
      return {
        id: index + ix,
        label: `target: ${c}`,
      };
    });
  }

  /**
   * Returns a fake 'stacktrace' where every 'stackframe' is a word from the current line.
   */
  public stack(startFrame: number, endFrame: number): IRuntimeStack {
    const line = this.getLine();
    const words = this.getWords(this.currentLine, line);
    words.push({ name: "BOTTOM", line: -1, index: -1 }); // add a sentinel so that the stack is never empty...

    // if the line contains the word 'disassembly' we support to "disassemble" the line by adding an 'instruction' property to the stackframe
    const instruction = line.indexOf("disassembly") >= 0 ? this.instruction : undefined;

    const column = typeof this.currentColumn === "number" ? this.currentColumn : undefined;

    const frames: IRuntimeStackFrame[] = [];
    // every word of the current line becomes a stack frame.
    for (let i = startFrame; i < Math.min(endFrame, words.length); i++) {
      const stackFrame: IRuntimeStackFrame = {
        index: i,
        name: `${words[i].name}(${i})`, // use a word of the line as the stackframe name
        file: this._sourceFile,
        line: this.currentLine,
        column: column, // words[i].index
        instruction: instruction ? instruction + i : 0,
      };

      frames.push(stackFrame);
    }

    return {
      frames: frames,
      count: words.length,
    };
  }

  /*
   * Determine possible column breakpoint positions for the given line.
   * Here we return the start location of words with more than 8 characters.
   */
  public getBreakpoints(path: string, line: number): number[] {
    return this.getWords(line, this.getLine(line))
      .filter((w) => w.name.length > 8)
      .map((w) => w.index);
  }

  /*
   * Set breakpoint in file with given line.
   */
  public async setBreakPoint(path: string, line: number): Promise<IRuntimeBreakpoint> {
    path = this.normalizePathAndCasing(path);

    const bp: IRuntimeBreakpoint = {
      verified: false,
      line,
      id: this.breakpointId++,
    };
    let bps = this.breakPoints.get(path);
    if (!bps) {
      bps = new Array<IRuntimeBreakpoint>();
      this.breakPoints.set(path, bps);
    }
    bps.push(bp);

    await this.verifyBreakpoints(path);

    return bp;
  }

  /*
   * Clear breakpoint in file with given line.
   */
  public clearBreakPoint(path: string, line: number): IRuntimeBreakpoint | undefined {
    const bps = this.breakPoints.get(this.normalizePathAndCasing(path));
    if (bps) {
      const index = bps.findIndex((bp) => bp.line === line);
      if (index >= 0) {
        const bp = bps[index];
        bps.splice(index, 1);
        return bp;
      }
    }
    return undefined;
  }

  public clearBreakpoints(path: string): void {
    this.breakPoints.delete(this.normalizePathAndCasing(path));
  }

  public setDataBreakpoint(address: string, accessType: "read" | "write" | "readWrite"): boolean {
    const x = accessType === "readWrite" ? "read write" : accessType;

    const t = this.breakAddresses.get(address);
    if (t) {
      if (t !== x) {
        this.breakAddresses.set(address, "read write");
      }
    } else {
      this.breakAddresses.set(address, x);
    }
    return true;
  }

  public clearAllDataBreakpoints(): void {
    this.breakAddresses.clear();
  }

  public setExceptionsFilters(namedException: string | undefined, otherExceptions: boolean): void {
    this.namedException = namedException;
    this.otherExceptions = otherExceptions;
  }

  public setInstructionBreakpoint(address: number): boolean {
    this.instructionBreakpoints.add(address);
    return true;
  }

  public clearInstructionBreakpoints(): void {
    this.instructionBreakpoints.clear();
  }

  public async getGlobalVariables(cancellationToken?: () => boolean): Promise<RuntimeVariable[]> {
    let a: RuntimeVariable[] = [];

    for (let i = 0; i < 10; i++) {
      a.push(new RuntimeVariable(`global_${i}`, i));
      if (cancellationToken && cancellationToken()) {
        break;
      }
      await timeout(1000);
    }

    return a;
  }

  public getLocalVariables(): RuntimeVariable[] {
    return Array.from(this.variables, ([name, value]) => value);
  }

  public getLocalVariable(name: string): RuntimeVariable | undefined {
    return this.variables.get(name);
  }

  /**
   * Return words of the given address range as "instructions"
   */
  public disassemble(address: number, instructionCount: number): RuntimeDisassembledInstruction[] {
    const instructions: RuntimeDisassembledInstruction[] = [];

    for (let a = address; a < address + instructionCount; a++) {
      if (a >= 0 && a < this.instructions.length) {
        instructions.push({
          address: a,
          instruction: this.instructions[a].name,
          line: this.instructions[a].line,
        });
      } else {
        instructions.push({
          address: a,
          instruction: "nop",
        });
      }
    }

    return instructions;
  }

  // private methods

  private getLine(line?: number): string {
    return this.sourceLines[line === undefined ? this.currentLine : line].trim();
  }

  private getWords(l: number, line: string): Word[] {
    // break line into words
    const WORD_REGEXP = /[a-z]+/gi;
    const words: Word[] = [];
    let match: RegExpExecArray | null;
    while ((match = WORD_REGEXP.exec(line))) {
      words.push({ name: match[0], line: l, index: match.index });
    }
    return words;
  }

  private async loadSource(file: string): Promise<void> {
    if (this._sourceFile !== file) {
      this._sourceFile = this.normalizePathAndCasing(file);
      this.initializeContents(await this.fileAccessor.readFile(file));
    }
  }

  private initializeContents(memory: Uint8Array) {
    this.sourceLines = new TextDecoder().decode(memory).split(/\r?\n/);

    this.instructions = [];

    this.starts = [];
    this.instructions = [];
    this.ends = [];

    for (let l = 0; l < this.sourceLines.length; l++) {
      this.starts.push(this.instructions.length);
      const words = this.getWords(l, this.sourceLines[l]);
      for (let word of words) {
        this.instructions.push(word);
      }
      this.ends.push(this.instructions.length);
    }
  }

  /**
   * return true on stop
   */
  private findNextStatement(reverse: boolean, stepEvent?: string): boolean {
    for (
      let ln = this.currentLine;
      reverse ? ln >= 0 : ln < this.sourceLines.length;
      reverse ? ln-- : ln++
    ) {
      // is there a source breakpoint?
      const breakpoints = this.breakPoints.get(this._sourceFile);
      if (breakpoints) {
        const bps = breakpoints.filter((bp) => bp.line === ln);
        if (bps.length > 0) {
          // send 'stopped' event
          this.sendEvent("stopOnBreakpoint");

          // the following shows the use of 'breakpoint' events to update properties of a breakpoint in the UI
          // if breakpoint is not yet verified, verify it now and send a 'breakpoint' update event
          if (!bps[0].verified) {
            bps[0].verified = true;
            this.sendEvent("breakpointValidated", bps[0]);
          }

          this.currentLine = ln;
          return true;
        }
      }

      const line = this.getLine(ln);
      if (line.length > 0) {
        this.currentLine = ln;
        break;
      }
    }
    if (stepEvent) {
      this.sendEvent(stepEvent);
      return true;
    }
    return false;
  }

  /**
   * "execute a line" of the readme markdown.
   * Returns true if execution sent out a stopped event and needs to stop.
   */
  private executeLine(ln: number, reverse: boolean): boolean {
    // first "execute" the instructions associated with this line and potentially hit instruction breakpoints
    while (reverse ? this.instruction >= this.starts[ln] : this.instruction < this.ends[ln]) {
      reverse ? this.instruction-- : this.instruction++;
      if (this.instructionBreakpoints.has(this.instruction)) {
        this.sendEvent("stopOnInstructionBreakpoint");
        return true;
      }
    }

    const line = this.getLine(ln);

    // find variable accesses
    let reg0 = /\$([a-z][a-z0-9]*)(=(false|true|[0-9]+(\.[0-9]+)?|\".*\"|\{.*\}))?/gi;
    let matches0: RegExpExecArray | null;
    while ((matches0 = reg0.exec(line))) {
      if (matches0.length === 5) {
        let access: string | undefined;

        const name = matches0[1];
        const value = matches0[3];

        let v = new RuntimeVariable(name, value);

        if (value && value.length > 0) {
          if (value === "true") {
            v.value = true;
          } else if (value === "false") {
            v.value = false;
          } else if (value[0] === '"') {
            v.value = value.slice(1, -1);
          } else if (value[0] === "{") {
            v.value = [
              new RuntimeVariable("fBool", true),
              new RuntimeVariable("fInteger", 123),
              new RuntimeVariable("fString", "hello"),
              new RuntimeVariable("flazyInteger", 321),
            ];
          } else {
            v.value = parseFloat(value);
          }

          if (this.variables.has(name)) {
            // the first write access to a variable is the "declaration" and not a "write access"
            access = "write";
          }
          this.variables.set(name, v);
        } else {
          if (this.variables.has(name)) {
            // variable must exist in order to trigger a read access
            access = "read";
          }
        }

        const accessType = this.breakAddresses.get(name);
        if (access && accessType && accessType.indexOf(access) >= 0) {
          this.sendEvent("stopOnDataBreakpoint", access);
          return true;
        }
      }
    }

    // if 'log(...)' found in source -> send argument to debug console
    const reg1 = /(log|prio|out|err)\(([^\)]*)\)/g;
    let matches1: RegExpExecArray | null;
    while ((matches1 = reg1.exec(line))) {
      if (matches1.length === 3) {
        this.sendEvent("output", matches1[1], matches1[2], this._sourceFile, ln, matches1.index);
      }
    }

    // if pattern 'exception(...)' found in source -> throw named exception
    const matches2 = /exception\((.*)\)/.exec(line);
    if (matches2 && matches2.length === 2) {
      const exception = matches2[1].trim();
      if (this.namedException === exception) {
        this.sendEvent("stopOnException", exception);
        return true;
      } else {
        if (this.otherExceptions) {
          this.sendEvent("stopOnException", undefined);
          return true;
        }
      }
    } else {
      // if word 'exception' found in source -> throw exception
      if (line.indexOf("exception") >= 0) {
        if (this.otherExceptions) {
          this.sendEvent("stopOnException", undefined);
          return true;
        }
      }
    }

    // nothing interesting found -> continue
    return false;
  }

  private async verifyBreakpoints(path: string): Promise<void> {
    const bps = this.breakPoints.get(path);
    if (bps) {
      await this.loadSource(path);
      bps.forEach((bp) => {
        if (!bp.verified && bp.line < this.sourceLines.length) {
          const srcLine = this.getLine(bp.line);

          // if a line is empty or starts with '+' we don't allow to set a breakpoint but move the breakpoint down
          if (srcLine.length === 0 || srcLine.indexOf("+") === 0) {
            bp.line++;
          }
          // if a line starts with '-' we don't allow to set a breakpoint but move the breakpoint up
          if (srcLine.indexOf("-") === 0) {
            bp.line--;
          }
          // don't set 'verified' to true if the line contains the word 'lazy'
          // in this case the breakpoint will be verified 'lazy' after hitting it once.
          if (srcLine.indexOf("lazy") < 0) {
            bp.verified = true;
            this.sendEvent("breakpointValidated", bp);
          }
        }
      });
    }
  }

  private sendEvent(event: string, ...args: any[]): void {
    setTimeout(() => {
      this.emit(event, ...args);
    }, 0);
  }

  private normalizePathAndCasing(path: string) {
    if (this.fileAccessor.isWindows) {
      return path.replace(/\//g, "\\").toLowerCase();
    } else {
      return path.replace(/\\/g, "/");
    }
  }
}
