use std::sync::Arc;

/// Trait for values being taken.
pub trait TakeAs<T> {
    /// Takes the inner value if there is exactly one strong reference and
    /// clones it otherwise.
    fn take(self) -> T;
}

impl<T: Clone> TakeAs<T> for Arc<T> {
    fn take(self) -> T {
        match Arc::try_unwrap(self) {
            Ok(v) => v,
            Err(rc) => (*rc).clone(),
        }
    }
}
