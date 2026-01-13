
#import "../lib.typ": *

#let db = create_index(read("index.jsonl", encoding: none))
Result is #query(db, "goto_definition", (
  textDocument: (uri: "file:///dummy-root/s0.typ"),
  position: (line: 0, character: 5),
)).
