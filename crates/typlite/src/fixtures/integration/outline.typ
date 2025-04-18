

#outline()

== Heading 1

=== Heading 2

#link("https://example.com")[
  This is a link to example.com
]

Inline `code` has `back-ticks around` it.

```cs
using System.IO.Compression;

#pragma warning disable 414, 3021

namespace MyApplication
{
    [Obsolete("...")]
    class Program : IInterface
    {
        public static List<int> JustDoIt(int count)
        {
            Console.WriteLine($"Hello {Name}!");
            return new List<int>(new int[] { 1, 2, 3 })
        }
    }
}
```

Math inline: $E = m c^2$ and block:
$
  E = m c^2
$

- First item
- Second item
  + First sub-item
  + Second sub-item
    - First sub-sub-item

/ First term: First definition

#table(
  columns: (1em,) * 20,
  ..range(20).map(x => [#x]),
)
