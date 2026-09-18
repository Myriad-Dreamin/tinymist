#metadata((
  test_pattern: "my-test-",
  panic_pattern: "should-panic-",
)) <test-config>

#let my-test-custom() = {
  assert(true)
}

#let should-panic-custom() = {
  panic("custom panic")
}
