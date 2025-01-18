use super::ir::{ArchivedFlatModule, FlatModule};
use rkyv::de::deserializers::SharedDeserializeMap;
use rkyv::{AlignedVec, Deserialize};

pub enum RkyvStreamData<'a> {
    Aligned(&'a [u8]),
    Unaligned(AlignedVec),
}

impl<'a> RkyvStreamData<'a> {
    /// # Safety
    /// This function is unsafe because it creates a reference to the archived
    /// value without checking bounds
    pub unsafe fn unchecked_peek<T: rkyv::Archive + ?Sized>(&'a self) -> &'a T::Archived {
        rkyv::archived_root::<T>(self.as_ref())
    }
}

impl From<AlignedVec> for RkyvStreamData<'_> {
    #[inline]
    fn from(v: AlignedVec) -> Self {
        Self::Unaligned(v)
    }
}

impl<'a> From<&'a [u8]> for RkyvStreamData<'a> {
    #[inline]
    fn from(v: &'a [u8]) -> Self {
        if (v.as_ptr() as usize) % AlignedVec::ALIGNMENT != 0 {
            let mut aligned = AlignedVec::with_capacity(v.len());
            aligned.extend_from_slice(v);
            Self::Unaligned(aligned)
        } else {
            Self::Aligned(v)
        }
    }
}

impl AsRef<[u8]> for RkyvStreamData<'_> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Aligned(v) => v,
            Self::Unaligned(v) => v.as_slice(),
        }
    }
}

pub struct BytesModuleStream<'a> {
    data: RkyvStreamData<'a>,
}

impl<'a> BytesModuleStream<'a> {
    pub fn from_slice(v: &'a [u8]) -> Self {
        Self {
            data: RkyvStreamData::from(v),
        }
    }

    pub fn checkout(&self) -> &ArchivedFlatModule {
        rkyv::check_archived_root::<FlatModule>(self.data.as_ref()).unwrap()
    }

    pub fn checkout_owned(&self) -> FlatModule {
        let v = self.checkout();
        let mut dmap = SharedDeserializeMap::default();
        v.deserialize(&mut dmap).unwrap()
    }
}
