use ecow::EcoString;

use super::*;

/// A package in the git.
#[derive(Clone)]
pub struct GitClPack<P> {
    /// The namespace to mount.
    pub namespace: EcoString,
    /// The URL of the git.
    pub url: P,
}

impl<P: AsRef<str>> GitClPack<P> {
    /// Creates a new `GitClPack` instance.
    pub fn new(namespace: EcoString, url: P) -> Self {
        Self { namespace, url }
    }
}

impl<P: AsRef<str>> fmt::Debug for GitClPack<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitClPack({})", self.url.as_ref())
    }
}

impl<P: AsRef<str>> PackFs for GitClPack<P> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        let temp_dir = std::env::temp_dir();
        let temp_dir = temp_dir.join("tinymist/package-gitcl");

        tinymist_std::fs::paths::temp_dir_in(temp_dir, |temp_dir| {
            clone(self.url.as_ref(), temp_dir)?;

            Ok(DirPack::new(temp_dir).read_all(f))
        })
        .map_err(other)?
    }
}

impl<P: AsRef<str>> Pack for GitClPack<P> {}

fn clone(url: &str, dst: &Path) -> io::Result<()> {
    let mut cmd = gitcl();
    cmd.arg("clone").arg(url).arg(dst);
    let status = cmd.status()?;
    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!("git clone failed: {status}"),
        ));
    }
    Ok(())
}

fn gitcl() -> std::process::Command {
    std::process::Command::new("git")
}
