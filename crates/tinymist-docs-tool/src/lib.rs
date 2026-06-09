//! Helpers for generating documentation data from the resolved `typst-assets`
//! source.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use cargo_metadata::{MetadataCommand, Package};
use regex::Regex;
use serde::Serialize;
use ttf_parser::{Face, name_id};

const GENERATED_BY: &str = "cargo run --quiet --bin tinymist-docs-tool -- --output docs/tinymist/generated/compiler-settings-fonts.json";

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct EmbeddedFontInventory {
    generated_by: &'static str,
    typst_assets: TypstAssetsSource,
    fonts: Vec<EmbeddedFontEntry>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct TypstAssetsSource {
    version: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct EmbeddedFontEntry {
    family: String,
    display_family: String,
    style: String,
    full_name: String,
    display_name: String,
    postscript_name: Option<String>,
    file_name: String,
    relative_path: String,
}

/// Writes the generated embedded-font inventory JSON to disk or checks that an
/// existing file is up to date.
pub fn write_inventory_json(output: &Path, check: bool) -> Result<()> {
    let inventory = load_inventory()?;
    let rendered = serde_json::to_string_pretty(&inventory)? + "\n";

    if check {
        let existing = fs::read_to_string(output).with_context(|| {
            format!("failed to read existing inventory at {}", output.display())
        })?;
        if existing != rendered {
            bail!(
                "{} is not up to date. Run `{GENERATED_BY}`.",
                output.display()
            );
        }

        return Ok(());
    }

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    fs::write(output, rendered)
        .with_context(|| format!("failed to write inventory to {}", output.display()))?;

    Ok(())
}

fn load_inventory() -> Result<EmbeddedFontInventory> {
    let package = resolve_typst_assets_package()?;
    let manifest_path = package.manifest_path;
    let package_dir = manifest_path
        .as_std_path()
        .parent()
        .context("typst-assets manifest path has no parent directory")?;
    let source_path = package_dir.join("src/lib.rs");
    let source = fs::read_to_string(&source_path)
        .with_context(|| format!("failed to read {}", source_path.display()))?;
    let relative_paths = extract_font_relative_paths(&source)?;

    let mut fonts = relative_paths
        .into_iter()
        .map(|relative_path| build_font_entry(package_dir, &relative_path))
        .collect::<Result<Vec<_>>>()?;
    fonts.sort_by(|lhs, rhs| {
        lhs.family
            .cmp(&rhs.family)
            .then(lhs.full_name.cmp(&rhs.full_name))
            .then(lhs.file_name.cmp(&rhs.file_name))
    });

    Ok(EmbeddedFontInventory {
        generated_by: GENERATED_BY,
        typst_assets: TypstAssetsSource {
            version: package.version.to_string(),
        },
        fonts,
    })
}

fn resolve_typst_assets_package() -> Result<Package> {
    let workspace_manifest = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml");
    let metadata = MetadataCommand::new()
        .manifest_path(&workspace_manifest)
        .exec()
        .context("failed to resolve Cargo metadata for the tinymist workspace")?;

    let mut packages = metadata
        .packages
        .into_iter()
        .filter(|package| package.name == "typst-assets");
    let Some(package) = packages.next() else {
        bail!("failed to locate the resolved `typst-assets` package in Cargo metadata");
    };

    if let Some(extra) = packages.next() {
        bail!(
            "expected a single resolved `typst-assets` package, found at least {} and {}",
            package.version,
            extra.version
        );
    }

    Ok(package)
}

fn extract_font_relative_paths(source: &str) -> Result<Vec<String>> {
    let fn_re = Regex::new(
        r#"(?s)pub fn fonts\(\) -> impl Iterator<Item = &'static \[u8\]>\s*\{.*?#\[cfg\(feature = "fonts"\)\]\s*\[(?P<body>.*?)\]\s*\.into_iter\(\)"#,
    )
    .expect("valid fonts() regex");
    let asset_re =
        Regex::new(r#"asset!\("(?P<path>fonts/[^"]+)"\)"#).expect("valid font asset regex");

    let captures = fn_re
        .captures(source)
        .context("failed to locate the `typst-assets::fonts()` asset list in src/lib.rs")?;
    let body = captures
        .name("body")
        .context("failed to capture the `typst-assets::fonts()` asset list body")?
        .as_str();

    let relative_paths = asset_re
        .captures_iter(body)
        .map(|captures| captures["path"].to_owned())
        .collect::<Vec<_>>();
    if relative_paths.is_empty() {
        bail!("`typst-assets::fonts()` did not contain any embedded font assets");
    }

    Ok(relative_paths)
}

fn build_font_entry(package_dir: &Path, relative_path: &str) -> Result<EmbeddedFontEntry> {
    let absolute_path = package_dir.join("files").join(relative_path);
    let data = fs::read(&absolute_path)
        .with_context(|| format!("failed to read {}", absolute_path.display()))?;
    let face = Face::parse(&data, 0)
        .with_context(|| format!("failed to parse {}", absolute_path.display()))?;

    let family = preferred_name(&face, &[name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY])
        .with_context(|| {
            format!(
                "failed to read a family name from {}",
                absolute_path.display()
            )
        })?;
    let style = preferred_name(&face, &[name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY])
        .unwrap_or_else(|| "Regular".to_owned());
    let full_name =
        preferred_name(&face, &[name_id::FULL_NAME]).unwrap_or_else(|| format!("{family} {style}"));
    let file_name = PathBuf::from(relative_path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .context("font asset path has no file name")?;

    Ok(EmbeddedFontEntry {
        display_family: prettify_display_name(&family),
        display_name: prettify_display_name(&full_name),
        family,
        style,
        full_name,
        postscript_name: preferred_name(&face, &[name_id::POST_SCRIPT_NAME]),
        file_name,
        relative_path: relative_path.to_owned(),
    })
}

fn preferred_name(face: &Face<'_>, ids: &[u16]) -> Option<String> {
    for id in ids {
        if let Some(name) = face
            .names()
            .into_iter()
            .find(|name| name.name_id == *id && name.is_unicode())
            .and_then(|name| name.to_string())
        {
            let name = name.trim();
            if !name.is_empty() {
                return Some(name.to_owned());
            }
        }
    }

    None
}

fn prettify_display_name(name: &str) -> String {
    name.replace("NewComputerModern", "New Computer Modern ")
        .replace("BoldItalic", "Bold Italic")
        .replace("SemiboldItalic", "Semibold Italic")
        .replace('-', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::{extract_font_relative_paths, load_inventory};

    #[test]
    fn extracts_embedded_font_assets_from_source() {
        let source = r#"
pub fn fonts() -> impl Iterator<Item = &'static [u8]> {
    #[cfg(not(feature = "fonts"))]
    return [].into_iter();

    #[cfg(feature = "fonts")]
    [
        asset!("fonts/Example-Regular.otf"),
        asset!("fonts/Example-Bold.otf"),
    ]
    .into_iter()
}
"#;

        let paths = extract_font_relative_paths(source).expect("extract font paths");
        assert_eq!(
            paths,
            vec![
                "fonts/Example-Regular.otf".to_owned(),
                "fonts/Example-Bold.otf".to_owned()
            ]
        );
    }

    #[test]
    fn loads_the_current_typst_assets_inventory() {
        let inventory = load_inventory().expect("load inventory");
        assert!(!inventory.fonts.is_empty());
        assert!(
            inventory
                .fonts
                .iter()
                .any(|font| font.full_name == "Libertinus Serif Regular")
        );
        assert!(
            inventory
                .fonts
                .iter()
                .any(|font| font.file_name == "NewCMMath-Regular.otf")
        );
    }
}
