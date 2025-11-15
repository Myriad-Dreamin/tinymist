// Test method chaining and field access
#let obj = (
  field1: 10,
  field2: 20,
  method: (x) => x + 1
)

#let used_obj = obj
#let unused_obj = (a: 1)

#used_obj.field1
#(used_obj.method)(5)
