use std::{ops::DerefMut, sync::Arc};

use indexmap::IndexSet;
use reflexo_typst::debug_loc::SourceSpan;
use tokio::sync::RwLock;

#[derive(Debug)]
pub enum InternQuery<T> {
    Ok(Option<T>),
    UseAfterFree,
}

pub struct InternId {
    lifetime: u32,
    id: u32,
}

impl InternId {
    pub fn new(lifetime: usize, id: usize) -> Self {
        Self {
            lifetime: lifetime as u32,
            id: id as u32,
        }
    }

    fn to_u64(&self) -> u64 {
        ((self.lifetime as u64) << 32) | self.id as u64
    }

    fn from_u64(id: u64) -> Self {
        Self {
            lifetime: (id >> 32) as u32,
            id: (id & 0xffffffff) as u32,
        }
    }

    pub fn to_hex(&self) -> String {
        format!("{:x}", self.to_u64())
    }

    pub fn from_hex(hex: &str) -> Self {
        Self::from_u64(u64::from_str_radix(hex, 16).unwrap())
    }
}

/// Span interner
///
/// Interns spans and returns an intern id. Intern id can be converted to a
/// span. Clone of the interner is cheap, and the clone shares the same interned
/// spans.
#[derive(Clone, Default)]
pub struct SpanInterner {
    inner: Arc<RwLock<SpanInternerImpl>>,
}

impl SpanInterner {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(unused)]
    pub async fn reset(&self) {
        self.inner.write().await.reset();
    }

    pub async fn span_by_str(&self, str: &str) -> InternQuery<SourceSpan> {
        self.inner.read().await.span_by_str(str)
    }

    #[allow(unused)]
    pub async fn span(&self, id: InternId) -> InternQuery<SourceSpan> {
        self.inner.read().await.span(id)
    }

    #[allow(unused)]
    pub async fn intern(&self, span: SourceSpan) -> InternId {
        self.inner.write().await.intern(span)
    }

    pub async fn with_writer<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut SpanInternerImpl) -> R,
    {
        f(self.inner.write().await.deref_mut())
    }
}

pub struct SpanInternerImpl {
    lifetime: usize,
    span2id: IndexSet<(usize, SourceSpan)>,
}

impl Default for SpanInternerImpl {
    fn default() -> Self {
        Self::new()
    }
}

const GARAGE_COLLECT_THRESHOLD: usize = 30;

impl SpanInternerImpl {
    pub fn new() -> Self {
        Self {
            lifetime: 1,
            span2id: IndexSet::new(),
        }
    }

    pub fn reset(&mut self) {
        self.lifetime += 1;
        self.span2id
            .retain(|(id, _)| self.lifetime - id < GARAGE_COLLECT_THRESHOLD);
    }

    pub fn span_by_str(&self, str: &str) -> InternQuery<SourceSpan> {
        self.span(InternId::from_hex(str))
    }

    pub fn span(&self, id: InternId) -> InternQuery<SourceSpan> {
        if (id.lifetime as usize + GARAGE_COLLECT_THRESHOLD) <= self.lifetime {
            InternQuery::UseAfterFree
        } else {
            InternQuery::Ok(
                self.span2id
                    .get_index(id.id as usize)
                    .map(|(_, span)| span)
                    .copied(),
            )
        }
    }

    pub fn intern(&mut self, span: SourceSpan) -> InternId {
        let item = (self.lifetime, span);
        let (idx, _) = self.span2id.insert_full(item);
        // combine lifetime

        InternId::new(self.lifetime, idx)
    }
}
