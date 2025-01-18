use core::fmt;
use std::sync::OnceLock;

use parking_lot::Mutex;

/// Represent the result of an immutable query reference.
/// The compute function should be pure enough.
///
/// [`compute`]: Self::compute
/// [`compute_ref`]: Self::compute_ref
pub struct QueryRef<Res, Err, QueryContext = ()> {
    ctx: Mutex<Option<QueryContext>>,
    /// `None` means no value has been computed yet.
    cell: OnceLock<Result<Res, Err>>,
}

impl<T, E, QC> QueryRef<T, E, QC> {
    pub fn with_value(value: T) -> Self {
        let cell = OnceLock::new();
        cell.get_or_init(|| Ok(value));
        Self {
            ctx: Mutex::new(None),
            cell,
        }
    }

    pub fn with_context(ctx: QC) -> Self {
        Self {
            ctx: Mutex::new(Some(ctx)),
            cell: OnceLock::new(),
        }
    }
}

impl<T, E: Clone, QC> QueryRef<T, E, QC> {
    /// Compute and return a checked reference guard.
    #[inline]
    pub fn compute<F: FnOnce() -> Result<T, E>>(&self, f: F) -> Result<&T, E> {
        self.compute_with_context(|_| f())
    }

    /// Compute with context and return a checked reference guard.
    #[inline]
    pub fn compute_with_context<F: FnOnce(QC) -> Result<T, E>>(&self, f: F) -> Result<&T, E> {
        let result = self.cell.get_or_init(|| f(self.ctx.lock().take().unwrap()));
        result.as_ref().map_err(Clone::clone)
    }

    /// Gets the reference to the (maybe uninitialized) result.
    ///
    /// Returns `None` if the cell is empty, or being initialized. This
    /// method never blocks.
    ///
    /// It is possible not hot, so that it is non-inlined
    pub fn get_uninitialized(&self) -> Option<&Result<T, E>> {
        self.cell.get()
    }
}

impl<T, E> Default for QueryRef<T, E> {
    fn default() -> Self {
        QueryRef {
            ctx: Mutex::new(Some(())),
            cell: OnceLock::new(),
        }
    }
}

impl<T, E, QC> fmt::Debug for QueryRef<T, E, QC>
where
    T: fmt::Debug,
    E: fmt::Debug,
    QC: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ctx = self.ctx.lock();
        let res = self.cell.get();
        f.debug_struct("QueryRef")
            .field("context", &ctx)
            .field("result", &res)
            .finish()
    }
}
