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
    async executeCommand<R>(command: string, args: any[]) {
        return await client!.sendRequest<R>("workspace/executeCommand", {
            command,
            arguments: args,
        });
    },
    getResource<T extends keyof ResourceRoutes>(path: T) {
        return this.executeCommand<ResourceRoutes[T]>("tinymist.getResources", [path]);
    },
};
