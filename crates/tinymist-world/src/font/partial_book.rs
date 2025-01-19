use core::fmt;

use typst::text::{FontFlags, FontInfo, FontVariant};

use crate::font::FontSlot;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct FontInfoKey {
    pub family: String,
    pub variant: FontVariant,
    pub flags: FontFlags,
}

impl From<&FontInfo> for FontInfoKey {
    fn from(info: &FontInfo) -> Self {
        Self {
            family: info.family.clone(),
            variant: info.variant,
            flags: info.flags,
        }
    }
}

#[derive(Debug, Default)]
pub struct PartialFontBook {
    pub revision: usize,
    pub partial_hit: bool,
    pub changes: Vec<(Option<usize>, FontInfo, FontSlot)>,
}

impl PartialFontBook {
    pub fn push(&mut self, change: (Option<usize>, FontInfo, FontSlot)) {
        self.partial_hit = true;
        self.changes.push(change);
    }
}

impl fmt::Display for PartialFontBook {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for (idx, info, slot) in &self.changes {
            writeln!(
                f,
                "{:?}: {} -> {:?}\n",
                idx,
                info.family,
                slot.get_uninitialized()
            )?;
        }

        Ok(())
    }
}
