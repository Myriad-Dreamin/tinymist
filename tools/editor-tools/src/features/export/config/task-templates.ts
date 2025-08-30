import type { ExportConfig, TaskDefinition } from "../types";

// VSCode variable constants to avoid template string linting issues
const VSCODE_VARS = {
  FILE: "$" + "{file}",
  FILE_BASENAME: "$" + "{fileBasename}",
  FILE_BASENAME_NO_EXT: "$" + "{fileBasenameNoExtension}",
  FILE_DIRNAME: "$" + "{fileDirname}",
  FILE_EXTNAME: "$" + "{fileExtname}",
  WORKSPACE_FOLDER: "$" + "{workspaceFolder}",
  WORKSPACE_FOLDER_BASENAME: "$" + "{workspaceFolderBasename}",
  CWD: "$" + "{cwd}",
  LINE_NUMBER: "$" + "{lineNumber}",
  SELECTED_TEXT: "$" + "{selectedText}"
} as const;

export function generateTaskDefinition(config: ExportConfig): TaskDefinition {
  const { format, options } = config;

  // Convert options to task export format
  const exportOptions: Record<string, string | number | boolean | undefined> = {
    format: format.id,
    inputPath: VSCODE_VARS.FILE,
    outputPath: generateOutputPath(format.fileExtension),
    ...options
  };

  return {
    type: 'typst',
    command: 'export',
    label: `Export to ${format.label}`,
    group: 'build',
    export: exportOptions
  };
}

function generateOutputPath(extension: string): string {
  return VSCODE_VARS.FILE_DIRNAME + "/" + VSCODE_VARS.FILE_BASENAME_NO_EXT + "." + extension;
}

export function generateTaskLabel(format: string, customLabel?: string): string {
  if (customLabel) {
    return customLabel;
  }
  return `Export to ${format}`;
}

// Common task variable substitutions for VSCode
export const TASK_VARIABLES = {
  file: VSCODE_VARS.FILE,
  fileBasename: VSCODE_VARS.FILE_BASENAME,
  fileBasenameNoExtension: VSCODE_VARS.FILE_BASENAME_NO_EXT,
  fileDirname: VSCODE_VARS.FILE_DIRNAME,
  fileExtname: VSCODE_VARS.FILE_EXTNAME,
  workspaceFolder: VSCODE_VARS.WORKSPACE_FOLDER,
  workspaceFolderBasename: VSCODE_VARS.WORKSPACE_FOLDER_BASENAME,
  cwd: VSCODE_VARS.CWD,
  lineNumber: VSCODE_VARS.LINE_NUMBER,
  selectedText: VSCODE_VARS.SELECTED_TEXT
};

export const COMMON_OUTPUT_PATTERNS = [
  {
    label: 'Same directory as source',
    value: VSCODE_VARS.FILE_DIRNAME + "/" + VSCODE_VARS.FILE_BASENAME_NO_EXT
  },
  {
    label: 'Output subdirectory',
    value: VSCODE_VARS.FILE_DIRNAME + "/output/" + VSCODE_VARS.FILE_BASENAME_NO_EXT
  },
  {
    label: 'Build directory',
    value: VSCODE_VARS.WORKSPACE_FOLDER + "/build/" + VSCODE_VARS.FILE_BASENAME_NO_EXT
  },
  {
    label: 'Dist directory',
    value: VSCODE_VARS.WORKSPACE_FOLDER + "/dist/" + VSCODE_VARS.FILE_BASENAME_NO_EXT
  }
];
