/// path: theme.tmTheme
not-a-real-theme

-----
/// path: syntax.tmLanguage
not-a-real-syntax

-----
/// path: img.png
not-a-real-png

-----
/// contains: +syntax.tmLanguage, -theme.tmTheme, -img.png
#let p = ""/* range -1..0 */
#raw(syntaxes: p)[hello]

