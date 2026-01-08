// Test usage in arrays and dictionaries
#let used_in_array = 1
#let used_in_dict = 2
#let unused = 3

#let arr = (used_in_array, 10, 20)
#let dict = (key: used_in_dict, other: 30)

#arr
#dict
