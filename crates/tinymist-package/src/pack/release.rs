use ecow::EcoString;

use super::*;

/// A package in the GitHub releases.
#[derive(Debug, Clone)]
pub struct GitHubReleasePack {
    /// The package specifier.
    pub specifier: PackageSpec,
    /// The URL of the package.
    pub repo: EcoString,
    /// The name of the package.
    pub name: EcoString,
}

impl PackFs for GitHubReleasePack {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest/{}",
            self.repo, self.name,
        );

        HttpPack::new(self.specifier.clone(), url).read_all(f)
    }
}

impl Pack for GitHubReleasePack {}
impl PackExt for GitHubReleasePack {}
