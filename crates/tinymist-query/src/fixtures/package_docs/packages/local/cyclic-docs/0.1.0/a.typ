/// Calls the documented function from module B.
///
/// - value (int): The value to pass through the cycle.
/// - scale (int): A scale applied before calling module B.
/// -> int
#let alpha(value, scale: 1) = {
  import "b.typ": beta
  beta(value * scale)
}
