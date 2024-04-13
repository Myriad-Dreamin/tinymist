#[derive(Debug, Clone, Hash)]
pub(crate) enum PathPreference {
    None,
    Source,
    Image,
    Json,
    Yaml,
    Xml,
    Toml,
}

impl PathPreference {
    pub(crate) fn match_ext(&self, ext: &std::ffi::OsStr) -> bool {
        let ext = || ext.to_str().map(|e| e.to_lowercase()).unwrap_or_default();

        match self {
            PathPreference::None => true,
            PathPreference::Source => {
                matches!(ext().as_ref(), "typ")
            }
            PathPreference::Image => {
                matches!(
                    ext().as_ref(),
                    "png" | "webp" | "jpg" | "jpeg" | "svg" | "svgz"
                )
            }
            PathPreference::Json => {
                matches!(ext().as_ref(), "json" | "jsonc" | "json5")
            }
            PathPreference::Yaml => matches!(ext().as_ref(), "yaml" | "yml"),
            PathPreference::Xml => matches!(ext().as_ref(), "xml"),
            PathPreference::Toml => matches!(ext().as_ref(), "toml"),
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBuiltinType {
    Args,
    Stroke,
    MarginLike,
    FillColor,
    TextSize,
    TextFont,
    DirParam,
    Path(PathPreference),
}

// "paint",
// "thickness",
// "cap",
// "join",
// "miter_limit",
// "dash",
// "dash",
// "miter-limit",
