import { Uri } from "vscode";
import { readFile } from "fs";

export default (uris: Uri[]|Thenable<Uri[]>, encoding: BufferEncoding): Thenable<string[]> => {
    return Promise.resolve(uris).then(uris => {
        return Promise.all(uris.map(uri => new Promise<string>((resolve, reject) => {
            readFile(uri.fsPath, encoding, (err, data) => {
                if (err) {
                    reject(err);
                } else {
                    resolve(data.toString());
                }
            });
        })));
    });
};
