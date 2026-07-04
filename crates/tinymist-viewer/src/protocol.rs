//! Preview data-plane protocol handling for the Vello viewer.

use tinymist_preview::protocol::{PreviewDataFrame, split_preview_data_frame};

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

#[cfg(test)]
mod tests {
    use tinymist_preview::protocol::{DIFF_V1_PREFIX, NEW_PREFIX};

    use super::preview_update_from_bytes;

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
}
