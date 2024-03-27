// read typst.tmLanguage.json

const fs = require("fs");
const path = require("path");
{
  const filePath = path.join(__dirname, "../typst.tmLanguage.json");

  const data = fs.readFileSync(filePath, "utf8");

  const json = JSON.parse(data);

  // todo: make it back when we finished
  // delete json.repository.fenced_code_block_typst.patterns;

  const outPath = path.join(
    __dirname,
    "../../../editors/vscode/out/typst.tmLanguage.json"
  );

  fs.writeFileSync(outPath, JSON.stringify(json, null, 4), "utf8");
}
{
  const filePath = path.join(__dirname, "../typst-code.tmLanguage.json");

  const data = fs.readFileSync(filePath, "utf8");

  const json = JSON.parse(data);

  // todo: make it back when we finished
  // delete json.repository.fenced_code_block_typst.patterns;

  const outPath = path.join(
    __dirname,
    "../../../editors/vscode/out/typst-code.tmLanguage.json"
  );

  fs.writeFileSync(outPath, JSON.stringify(json, null, 4), "utf8");
}
