/// path: theme.tmTheme
not-a-real-theme

-----
/// path: syntax.tmLanguage
not-a-real-syntax

-----
/// path: img.png
not-a-real-png

-----
/// contains: +theme.tmTheme, -syntax.tmLanguage, -img.png
#let p = ""/* range -1..0 */
#raw(theme: p)[hello]

