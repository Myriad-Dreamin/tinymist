//! Preview data-plane protocol handling for the Vello viewer.

/// Prefix for incremental vector document updates.
pub const DIFF_V1_PREFIX: &[u8] = b"diff-v1,";

/// Prefix for a full current vector document update.
pub const NEW_PREFIX: &[u8] = b"new,";

/// A preview data-plane frame with its prefix stripped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewDataFrame<'a> {
    /// An incremental update.
    DiffV1(&'a [u8]),
    /// A full current update.
    FullCurrent(&'a [u8]),
}

/// Splits a preview data-plane binary frame into its event kind and payload.
fn split_preview_data_frame(bytes: &[u8]) -> Option<PreviewDataFrame<'_>> {
    if let Some(payload) = bytes.strip_prefix(DIFF_V1_PREFIX) {
        Some(PreviewDataFrame::DiffV1(payload))
    } else {
        bytes
            .strip_prefix(NEW_PREFIX)
            .map(PreviewDataFrame::FullCurrent)
    }
}

/// A preview vector document update ready to merge into the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewUpdate<'a> {
    /// Whether the viewer must reset client-side state before merging.
    pub reset_before_merge: bool,
    /// The serialized vector document payload without the data-plane prefix.
    pub payload: &'a [u8],
}

/// Parses a preview data-plane frame into a viewer update.
pub fn preview_update_from_bytes(bytes: &[u8]) -> Option<PreviewUpdate<'_>> {
    match split_preview_data_frame(bytes)? {
        PreviewDataFrame::DiffV1(payload) => Some(PreviewUpdate {
            reset_before_merge: false,
            payload,
        }),
        PreviewDataFrame::FullCurrent(payload) => Some(PreviewUpdate {
            reset_before_merge: true,
            payload,
        }),
    }
}

/// Converts an incremental frame into a full-current frame.
pub fn full_current_frame_from_delta(delta: &[u8]) -> Option<Vec<u8>> {
    let payload = delta.strip_prefix(DIFF_V1_PREFIX)?;
    Some([NEW_PREFIX, payload].concat())
}

#[cfg(test)]
mod tests {
    use super::{
        DIFF_V1_PREFIX, NEW_PREFIX, full_current_frame_from_delta, preview_update_from_bytes,
    };

    #[test]
    fn diff_v1_frame_merges_without_reset() {
        let frame = [DIFF_V1_PREFIX, b"x"].concat();

        let update = preview_update_from_bytes(&frame).expect("diff-v1 should be accepted");

        assert!(!update.reset_before_merge);
        assert_eq!(update.payload, b"x");
    }

    #[test]
    fn full_current_frame_resets_before_merge() {
        let frame = [NEW_PREFIX, b"x"].concat();

        let update = preview_update_from_bytes(&frame).expect("new should be accepted");

        assert!(update.reset_before_merge);
        assert_eq!(update.payload, b"x");
    }

    #[test]
    fn diff_v1_frame_can_be_converted_to_full_current() {
        let frame = [DIFF_V1_PREFIX, b"x"].concat();

        let current = full_current_frame_from_delta(&frame).expect("diff-v1 should convert");

        assert_eq!(current, [NEW_PREFIX, b"x"].concat());
    }
}
