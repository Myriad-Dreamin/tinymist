/// #let fn = `(..fn-args) => any`;
///
/// - fn (function, fn): The `fn`.
/// - max-repetitions (int): The `max-repetitions`.
/// - repetitions (int): The `repetitions`.
/// - args (any, fn-args): The `args`.
#let touying-fn-wrapper(fn, max-repetitions: none, repetitions: none, ..args) = none

#(/* ident after */ touying-fn-wrapper);
