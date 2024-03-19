// read typst.tmLanguage.json

const fs = require("fs");
const path = require("path");

const filePath = path.join(__dirname, "../typst.tmLanguage.json");

const data = fs.readFileSync(filePath, "utf8");

const json = JSON.parse(data);

json.scopeName = "source.typst-grammar";
json.name = "typst-grammar";
json.repository.fenced_code_block_typst.patterns = [
  { include: "source.typst-grammar" },
];

const outPath = path.join(
  __dirname,
  "../../../editors/vscode/out/typst.tmLanguage.json"
);

fs.writeFileSync(outPath, JSON.stringify(json, null, 4), "utf8");
