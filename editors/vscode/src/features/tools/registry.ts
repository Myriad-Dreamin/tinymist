import type { EditorTool } from "./index";
import templateGallery from "./tools/template-gallery";
import summary from "./tools/summary";
// Import other tools as they are created
// import tracing from "./tracing";
// import profileServer from "./profile-server";
// import fontView from "./font-view";
// import symbolView from "./symbol-view";
// import docsView from "./docs-view";

export class ToolRegistry {
  private static instance: ToolRegistry;
  private tools: Map<string, EditorTool> = new Map();

  private constructor() {
    this.registerDefaultTools();
  }

  static getInstance(): ToolRegistry {
    if (!ToolRegistry.instance) {
      ToolRegistry.instance = new ToolRegistry();
    }
    return ToolRegistry.instance;
  }

  private registerDefaultTools(): void {
    this.registerTool(templateGallery);
    this.registerTool(summary);
    // Register other tools as they are created
    // this.registerTool(tracing);
    // this.registerTool(profileServer);
    // this.registerTool(fontView);
    // this.registerTool(symbolView);
    // this.registerTool(docsView);
  }

  registerTool(tool: EditorTool): void {
    this.tools.set(tool.id, tool);
  }

  getTool(id: string): EditorTool | undefined {
    return this.tools.get(id);
  }

  getAllTools(): EditorTool[] {
    return Array.from(this.tools.values());
  }

  getToolsWithCommands(): EditorTool[] {
    return this.getAllTools().filter(tool => tool.command);
  }
}
