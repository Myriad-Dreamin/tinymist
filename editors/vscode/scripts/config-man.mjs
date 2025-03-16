import * as fs from "fs";
import * as path from "path";

import { vscodeExtTranslations } from "../../../scripts/build-l10n.mjs";

const projectRoot = path.join(import.meta.dirname, "../../..");

const packageJsonPath = path.join(projectRoot, "editors/vscode/package.json");
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, "utf8"));

const otherPackageJsonPath = path.join(projectRoot, "editors/vscode/package.other.json");
const otherPackageJson = JSON.parse(fs.readFileSync(otherPackageJsonPath, "utf8"));

const config = packageJson.contributes.configuration.properties;
const otherConfig = otherPackageJson.contributes.configuration.properties;

const translate = (desc) => {
  const translations = vscodeExtTranslations["en"];
  desc = desc.replace(/\%(.*?)\%/g, (_, key) => {
    if (!translations[key]) {
      throw new Error(`Missing translation for ${key}`);
    }
    return translations[key];
  });
  return desc;
};

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
  const initPath = path.join(projectRoot, "crates/tinymist/src/config.rs");
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
const isServerSideConfig = (key, isOther) => {
  if (
    !(
      serverSideKeys.includes(key) ||
      serverSideKeys.some((serverSideKey) => key.startsWith(`${serverSideKey}.`))
    )
  ) {
    return false;
  }

  if (key.startsWith("tinymist.preview") && !isOther) {
    return false;
  }

  return true;
};
const configMd = (editor, prefix) => {
  const handleOne = (config, key, isOther) => {
    const {
      description: rawDescription,
      markdownDescription,
      default: dv,
      type: itemType,
      enum: enumBase,
      enumDescriptions: enumBaseDescription,
      markdownDeprecationMessage,
    } = config[key];

    const description = translate(markdownDescription || rawDescription);

    if (markdownDeprecationMessage) {
      return;
    }

    let defaultValue = dv;
    if (editor !== "vscode") {
      if (key === "tinymist.compileStatus") {
        defaultValue = "disable";
      }

      if (!isServerSideConfig(key, isOther)) {
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
          enumSections.push(`  - \`${enumBase[i]}\`: ${translate(enumBaseDescription[i])}`);
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
  };

  const vscodeConfigs = Object.keys(config).map((key) => handleOne(config, key, false));
  const otherConfigs = Object.keys(otherConfig).map((key) => handleOne(otherConfig, key, true));
  return [...vscodeConfigs, ...(editor === "vscode" ? [] : otherConfigs)]
    .filter((x) => x)
    .join("\n");
};

const configMdPath = path.join(import.meta.dirname, "..", "Configuration.md");

fs.writeFileSync(
  configMdPath,
  `# Tinymist Server Configuration

${configMd("vscode", true)}`,
);

const configMdPathNeovim = path.join(
  import.meta.dirname,
  "../../../editors/neovim/Configuration.md",
);

fs.writeFileSync(
  configMdPathNeovim,
  `# Tinymist Server Configuration

${configMd("neovim", false)}`,
);
