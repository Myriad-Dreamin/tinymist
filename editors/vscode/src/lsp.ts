import { LanguageClient } from "vscode-languageclient/node";
import * as vscode from "vscode";

export let client: LanguageClient | undefined = undefined;
export function setClient(newClient: LanguageClient) {
    client = newClient;
}

interface ResourceRoutes {
    "/symbols": any;
    "/preview/index.html": string;
}

export const tinymist = {
    getResource<T extends keyof ResourceRoutes>(path: T) {
        return vscode.commands.executeCommand<ResourceRoutes[T]>("tinymist.getResources", path);
    },
};
