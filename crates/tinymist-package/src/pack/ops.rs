use super::*;

/// A package in the directory.
pub struct FilterPack<'a, Src, F> {
    /// The files storing the package.
    pub(crate) src: &'a mut Src,
    /// The filter function to apply to each file.
    pub(crate) f: F,
}

impl<S: PackFs, F> fmt::Debug for FilterPack<'_, S, F> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FilterPack({:?}, ..)", self.src)
    }
}
impl<Src: PackFs, F: Fn(&str) -> bool + Send + Sync> PackFs for FilterPack<'_, Src, F> {
    fn read_all(
        &mut self,
        f: &mut (dyn FnMut(&str, PackFile) -> PackageResult<()> + Send + Sync),
    ) -> PackageResult<()> {
        self.src.read_all(&mut |path, file| {
            if (self.f)(path) {
                f(path, file)
            } else {
                Ok(())
            }
        })
    }
}

impl<Src: PackFs, F: Fn(&str) -> bool + Send + Sync> Pack for FilterPack<'_, Src, F> {}
