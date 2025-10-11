use crate::error::WriteResult;
use ecow::EcoString;

/// Formatting policy centralising newline and blank-line handling for CommonMark emission.
#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct FormatPolicy;

impl FormatPolicy {
    /// Ensure the buffer ends with a single trailing newline.
    pub fn ensure_trailing_newline(&mut self, buffer: &mut EcoString) -> WriteResult<()> {
        if !buffer.ends_with('\n') {
            buffer.push('\n');
        }
        Ok(())
    }

    /// Ensure the buffer ends with an empty line (two trailing newlines).
    pub fn ensure_blank_line(&mut self, buffer: &mut EcoString) -> WriteResult<()> {
        self.ensure_trailing_newline(buffer)?;
        if trailing_newline_count(buffer) < 2 {
            buffer.push('\n');
        }
        Ok(())
    }

    /// Apply spacing rules between consecutive nodes when traversing a block sequence.
    pub fn prepare_block_sequence(
        &mut self,
        buffer: &mut EcoString,
        previous_was_block: bool,
        next_is_block: bool,
    ) -> WriteResult<()> {
        if previous_was_block && next_is_block {
            self.ensure_blank_line(buffer)
        } else if previous_was_block || next_is_block {
            self.ensure_trailing_newline(buffer)
        } else {
            Ok(())
        }
    }
}

fn trailing_newline_count(buffer: &EcoString) -> usize {
    buffer.chars().rev().take_while(|&ch| ch == '\n').count()
}
