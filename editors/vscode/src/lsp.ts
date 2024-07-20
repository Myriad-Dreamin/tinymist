import { LanguageClient } from "vscode-languageclient/node";

export let client: LanguageClient | undefined = undefined;

export function setClient(newClient: LanguageClient) {
    client = newClient;
    clientPromiseResolve(newClient);
}

let clientPromiseResolve = (_client: LanguageClient) => {};
let clientPromise: Promise<LanguageClient> = new Promise((resolve) => {
    clientPromiseResolve = resolve;
});
export async function getClient(): Promise<LanguageClient> {
    return clientPromise;
}

interface ResourceRoutes {
    "/symbols": any;
    "/preview/index.html": string;
}

export const tinymist = {
    async executeCommand<R>(command: string, args: any[]) {
        return await (
            await getClient()
        ).sendRequest<R>("workspace/executeCommand", {
            command,
            arguments: args,
        });
    },
    getResource<T extends keyof ResourceRoutes>(path: T) {
        return this.executeCommand<ResourceRoutes[T]>("tinymist.getResources", [path]);
    },
};
