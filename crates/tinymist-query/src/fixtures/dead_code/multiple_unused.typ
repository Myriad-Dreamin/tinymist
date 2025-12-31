// Test multiple unused definitions
#let unused1 = 1
#let unused2 = 2
#let used = 3
#let unused3 = 4

#let unused_func1() = "a"
#let used_func() = "b"
#let unused_func2() = "c"

#used
#used_func()
