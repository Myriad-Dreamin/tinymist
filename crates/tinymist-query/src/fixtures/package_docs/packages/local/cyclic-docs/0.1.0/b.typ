/// Calls the documented function from module A.
///
/// - value (int): The value to pass through the cycle.
/// - offset (int): An offset applied before calling module A.
/// -> int
#let beta(value, offset: 0) = {
  import "a.typ": alpha
  alpha(value + offset)
}

/// Selects a documented function through module A.
///
/// - value (int): The value to pass through the module cycle.
/// -> int
#let beta-module(value) = {
  import "a.typ" as a
  a.alpha-module(value)
}
