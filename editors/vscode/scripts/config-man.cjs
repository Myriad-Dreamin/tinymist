const fs = require("fs");
const path = require("path");

const projectRoot = path.join(__dirname, "../../..");
const packageJsonPath = path.join(projectRoot, "editors/vscode/package.json");

const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));

const config = packageJson.contributes.configuration.properties;

// Generate Configuration.md string

const describeType = (typeOrTypeArray) => {
  if (Array.isArray(typeOrTypeArray)) {
    // join with , and add 'or' before the last element
    typeOrTypeArray = typeOrTypeArray.map(describeType);
    return (
      typeOrTypeArray.slice(0, -1).join(", ") +
      (typeOrTypeArray.length > 1 ? " or " : "") +
      typeOrTypeArray.slice(-1)
    );
  }
  switch (typeOrTypeArray) {
    case "boolean":
      return "`boolean`";
    case "string":
      return "`string`";
    case "number":
      return "`number`";
    case "array":
      return "`array`";
    case "object":
      return "`object`";
    case "null":
      return "`null`";
    default:
      return "`unknown`";
  }
};

const matchRegion = (content, regionName) => {
    const reg = new RegExp(`// region ${regionName}([\\s\\S]*?)// endregion ${regionName}`, "gm");
    const match = reg.exec(content);
    if (!match) {
        throw new Error(`Failed to match region ${regionName}`);
    }
    return match[1];
};

const serverSideKeys = (() => {
    const initPath = path.join(projectRoot, "crates/tinymist/src/init.rs");
    const initContent = fs.readFileSync(initPath, "utf8");
    const configItemContent = matchRegion(initContent, "Configuration Items");
    const strReg = /"([^"]+)"/g;
    const strings = [];
    let strMatch;
    while ((strMatch = strReg.exec(configItemContent)) !== null) {
        strings.push(strMatch[1]);
    }
    return strings.map((x) => `tinymist.${x}`);
})();
const isServerSideConfig = (key) => serverSideKeys.includes(key);
const configMd = (editor, prefix) =>
  Object.keys(config)
    .map((key) => {
      const {
        description: rawDescription,
        markdownDescription,
        default: dv,
        type: itemType,
        enum: enumBase,
        enumDescriptions: enumBaseDescription,
        markdownDeprecationMessage,
      } = config[key];

      const description = markdownDescription || rawDescription;

      if (markdownDeprecationMessage) {
        return;
      }

      let defaultValue = dv;
      if (editor !== "vscode") {
        if (key === "tinymist.compileStatus") {
          defaultValue = "disable";
        }

        if (!isServerSideConfig(key)) {
          return;
        }
      }

      const keyWithoutPrefix = key.replace("tinymist.", "");
      const name = prefix ? `tinymist.${keyWithoutPrefix}` : keyWithoutPrefix;
      const typeSection = itemType ? `\n- **Type**: ${describeType(itemType)}` : "";
      const defaultSection = defaultValue
        ? `\n- **Default**: \`${JSON.stringify(defaultValue)}\``
        : "";
      const enumSections = [];
      if (enumBase) {
        // zip enum values and descriptions
        for (let i = 0; i < enumBase.length; i++) {
          if (enumBaseDescription?.[i]) {
            enumSections.push(`  - \`${enumBase[i]}\`: ${enumBaseDescription[i]}`);
          } else {
            enumSections.push(`  - \`${enumBase[i]}\``);
          }
        }
      }
      const enumSection = enumSections.length ? `\n- **Enum**:\n${enumSections.join("\n")}` : "";

      return `## \`${name}\`

${description}
${typeSection}${enumSection}${defaultSection}
`;
    })
    .filter((x) => x)
    .join("\n");

const configMdPath = path.join(__dirname, "..", "Configuration.md");

fs.writeFileSync(
  configMdPath,
  `# Tinymist Server Configuration

${configMd("vscode", true)}`,
);

const configMdPathNeovim = path.join(__dirname, "../../../editors/neovim/Configuration.md");

fs.writeFileSync(
  configMdPathNeovim,
  `# Tinymist Server Configuration

${configMd("neovim", false)}`,
);
