// enum Preview Mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum PreviewMode {
    /// Preview mode for regular document
    #[cfg_attr(feature = "clap", clap(name = "document"))]
    Document,

    /// Preview mode for slide
    #[cfg_attr(feature = "clap", clap(name = "slide"))]
    Slide,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "clap", derive(clap::Parser))]
pub struct PreviewArgs {
    /// Data plane server will bind to this address
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "data-plane-host",
            default_value = "127.0.0.1:23625",
            value_name = "HOST",
            hide(true)
        )
    )]
    pub data_plane_host: String,

    /// Control plane server will bind to this address
    #[cfg_attr(
        feature = "clap",
        clap(
            long = "control-plane-host",
            default_value = "127.0.0.1:23626",
            value_name = "HOST",
            hide(true)
        )
    )]
    pub control_plane_host: String,

    /// Only render visible part of the document. This can improve performance
    /// but still being experimental.
    #[cfg_attr(feature = "clap", clap(long = "partial-rendering"))]
    pub enable_partial_rendering: bool,

    /// Invert colors of the preview (useful for dark themes without cost).
    /// Please note you could see the origin colors when you hover elements in
    /// the preview.
    #[cfg_attr(feature = "clap", clap(long, default_value = "never"))]
    pub invert_colors: String,
}
