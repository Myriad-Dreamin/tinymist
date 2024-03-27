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

const configMd = (prefix) =>
    Object.keys(config)
        .map((key) => {
            const {
                description,
                default: defaultValue,
                type: itemType,
                enum: enumBase,
                enumDescriptions: enumBaseDescription,
            } = config[key];
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
            const enumSection = enumSections.length
                ? `\n- **Enum**:\n${enumSections.join("\n")}`
                : "";

            return `## \`${name}\`

${description}
${typeSection}${enumSection}${defaultSection}
`;
        })
        .join("\n");

const configMdPath = path.join(__dirname, "..", "Configuration.md");

fs.writeFileSync(
    configMdPath,
    `# Tinymist Server Configuration

${configMd(true)}`
);

const configMdPathNeovim = path.join(__dirname, "../../../editors/neovim/Configuration.md");

fs.writeFileSync(
    configMdPathNeovim,
    `# Tinymist Server Configuration

${configMd(false)}`
);
