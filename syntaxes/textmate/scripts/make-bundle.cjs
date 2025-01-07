// read typst.tmLanguage.json

const fs = require("fs").promises;
const path = require("path");

/**
 * @typedef {{
 *   filePath: string;
 *   content: string;
 *   lineCount: number;
 * }} SourceFile
 *
 * @param {string} dirPath
 * @returns {Promise<SourceFile[]>}
 */
async function readFiles(dirPath) {
  // Scans all .typ files recursively
  /**
   * @returns {Promise<SourceFile[]>}
   */
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
          const content = await fs.readFile(filePath, "utf8");
          const lineCount = content.split("\n").length;
          typFiles.push({ filePath, content, lineCount });
        }
      })
    );

    return typFiles;
  };

  const files = await scanTypFiles(dirPath);
  // sort by file name
  files.sort((a, b) => {
    return a.filePath.localeCompare(b.filePath);
  });

  return files;
}

async function writeBundle(bundles, outPath) {
  // split bundle per 100000 lines
  const chunked = [];
  let chunk = [];
  let totalLineCount = 0;
  for (const file of bundles) {
    if (totalLineCount + file.lineCount > 100000) {
      chunked.push(chunk);
      chunk = [];
      totalLineCount = 0;
    }
    chunk.push(`// ${file.filePath}`);
    chunk.push(file.content);
    totalLineCount += file.lineCount;
  }
  if (chunk.length > 0) {
    chunked.push(chunk);
  }

  // write bundle
  await Promise.all(
    chunked.map(async (chunk, index) => {
      const chunkPath = outPath.replace(".typ", `-${index}.typ`);
      await fs.writeFile(chunkPath, chunk.join("\n")).then(() => {
        console.log(`Wrote ${chunkPath}`);
      });
    })
  );
}

async function main() {
  await fs.mkdir(path.join(__dirname, "../tests/bundles"), { recursive: true });

  {
    // const bundle = typFiles.join("\n");
    const bundles = await readFiles(path.join(__dirname, "../tests/packages"));
    const outPath = path.join(__dirname, "../tests/bundles/typst-packages.typ");
    await writeBundle(bundles, outPath);
  }
  {
    const bundles = await readFiles(
      path.join(__dirname, "../tests/official-testing")
    );
    const outPath = path.join(
      __dirname,
      "../tests/bundles/typst-official-testing.typ"
    );
    await writeBundle(bundles, outPath);
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
