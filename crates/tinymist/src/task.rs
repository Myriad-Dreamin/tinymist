/// A task that can be sent to the context (compiler/render thread)
///
/// The internal function will be dereferenced and called on the context.
pub type BorrowTask<Ctx> = Box<dyn FnOnce(&mut Ctx) + Send + 'static>;
