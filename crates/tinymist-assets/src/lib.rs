/// If this file is not found, please refer to https://enter-tainer.github.io/typst-preview/dev.html to build the frontend.
#[cfg(feature = "typst-preview")]
pub const HTML: &str = include_str!("typst-preview.html");
#[cfg(not(feature = "typst-preview"))]
pub const HTML: &str = "<html><body>Typst Preview needs to be built with the `embed-html` feature to work!</body></html>";
