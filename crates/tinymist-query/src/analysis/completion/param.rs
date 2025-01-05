//! Completion for param items.
//!
//! Note, this is used for the completion of parameters on a function's
//! *definition* instead of the completion of arguments of some *function call*.

use super::*;
impl CompletionPair<'_, '_, '_> {
    /// Complete parameters.
    pub fn complete_params(&mut self) -> bool {
        true
    }
}
