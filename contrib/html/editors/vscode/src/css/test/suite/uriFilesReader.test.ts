import uriFilesReader from "../../uriFilesReader";
import * as assert from "assert";
import { Uri } from "vscode";

suite("Test uriFilesReader", () => {
    test("Can't load http protocol", () => {
        const httpsUri = Uri.parse("https://maxcdn.bootstrapcdn.com/bootstrap/3.3.7/css/bootstrap.min.css");
        return uriFilesReader([httpsUri], "utf8").then((data: any) => {
            assert(false, "Expected promise to be rejected.");
        },(err: any) => {
            assert(err.code === "ENOENT");
        });
    });
});
