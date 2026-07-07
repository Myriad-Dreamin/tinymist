#set page(width: 10cm, height: auto)

= Hello, Typst!

This is a simple Typst document for testing.

#let highlight(content) = {
  text(fill: red, content)
}

#highlight[This text should be highlighted in red.]

- Item 1
- Item 2
- Item 3

```python
def hello():
    print("Hello, world!")
```