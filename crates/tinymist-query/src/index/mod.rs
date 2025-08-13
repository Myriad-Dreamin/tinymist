pub struct LsifIndex;

/// Generates [LSIF] index for the given package.
/// [LSIF]: https://microsoft.github.io/language-server-protocol/specifications/lsif/0.6.0/specification/
pub fn lsif_index(ctx: &mut LocalContext, spec: &PackageInfo) -> StrResult<LsifIndex> {
    todo!()
}
