use std::ops;

use super::typst_tokens::Modifier;

#[derive(Default, Clone, Copy)]
pub struct ModifierSet(u32);

impl ModifierSet {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn new(modifiers: &[Modifier]) -> Self {
        let bits = modifiers
            .iter()
            .copied()
            .map(Modifier::bitmask)
            .fold(0, |bits, mask| bits | mask);
        Self(bits)
    }

    pub fn bitset(self) -> u32 {
        self.0
    }
}

impl ops::BitOr for ModifierSet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}
