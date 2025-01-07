// read typst.tmLanguage.json

const fs = require("fs").promises;
const path = require("path");

/**
 * @param {string} dirPath
 */
async function makeBundle(dirPath) {
  // Scans all .typ files recursively
  const scanTypFiles = async (dirPath) => {
    const files = await fs.readdir(dirPath, { withFileTypes: true });
    const typFiles = [];
    await Promise.all(
      files.map(async (file) => {
        const filePath = path.join(dirPath, file.name);
        if (file.isDirectory()) {
          const children = await scanTypFiles(filePath);
          typFiles.push(...children);
        } else if (file.isFile() && file.name.endsWith(".typ")) {
          // push filename
          const filePath = path.join(dirPath, file.name).replace(/\\/g, "/");
          typFiles.push(`// ${filePath}`);
          typFiles.push(await fs.readFile(filePath, "utf8"));
        }
      })
    );

    return typFiles;
  };

  const typFiles = await scanTypFiles(dirPath);
  const bundle = typFiles.join("\n");
  return bundle;
}

async function main() {
  await fs.mkdir(path.join(__dirname, "../tests/bundles"), { recursive: true });

  {
    const bundle = await makeBundle(path.join(__dirname, "../tests/packages"));
    const outPath = path.join(__dirname, "../tests/bundles/typst-packages.typ");
    await fs.writeFile(outPath, bundle);
  }
  {
    const bundle = await makeBundle(
      path.join(__dirname, "../tests/official-testing")
    );
    const outPath = path.join(
      __dirname,
      "../tests/bundles/typst-official-testing.typ"
    );
    await fs.writeFile(outPath, bundle);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
