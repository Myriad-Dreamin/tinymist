//! Preview data-plane protocol helpers.

/// Prefix for incremental vector document updates.
pub const DIFF_V1_PREFIX: &[u8] = b"diff-v1,";

/// Prefix for a full current vector document update.
pub const NEW_PREFIX: &[u8] = b"new,";

/// A preview data-plane frame with its prefix stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewDataFrame<'a> {
    /// An incremental update.
    DiffV1(&'a [u8]),
    /// A full current update. Clients must reset their render state before
    /// merging the payload.
    FullCurrent(&'a [u8]),
}

/// Splits a preview data-plane binary frame into its event kind and payload.
pub fn split_preview_data_frame(bytes: &[u8]) -> Option<PreviewDataFrame<'_>> {
    if let Some(payload) = bytes.strip_prefix(DIFF_V1_PREFIX) {
        Some(PreviewDataFrame::DiffV1(payload))
    } else {
        bytes
            .strip_prefix(NEW_PREFIX)
            .map(PreviewDataFrame::FullCurrent)
    }
}
