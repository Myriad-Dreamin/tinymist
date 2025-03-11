#import "mod.typ": *

#show: book-page.with(title: [Code Documentation])

Tinymist will read the documentation from the source code and display it in the editor. For example, you can hover over a identifier to see its documentation, usually the content of the comments above the identifier's definition. The format of the documentation follows #link("https://github.com/typst-community/guidelines/pull/8")[this guideline].

*Note: the feature is not yet officially supported.*

== Status of the Feature

- #sym.checkmark Syntax of Docstring's Content: We have reached consensus on the syntax of content. It MUST be written in Typst.
- #sym.quest Annotations in Docstring's Content: We check the annotations in docstring by #link("https://typst.app/universe/package/tidy")[tidy style]. It's not an official standard.
- #sym.crossmark Syntax of Docstring: We haven't reached consensus on the syntax of docstring. It's not clear whether we should distinguish the docstring from regular comments.

== Format of Docstring

A docstring is an object in source code associating with some typst definition, whose content is the documentation information of the definition. Documentation is placed on consecutive special comments using three forward slashes `///` and an optional space. These are called doc comments.

While the #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/crates/tinymist-query/src/syntax/comment.rs")[`DocCommentMatcher`] matches doc comments in a looser way, we recommend using the strict syntax mentioned in the following sections.

=== Example 1

The content MUST follow typst syntax instead of markdown syntax.

```typ
/// You can use *typst markup* in docstring.
#let foo = 1;
```

Explanation: The documentation of `foo` is "You can use *typst markup* in docstring."

=== Example 2

The comments SHOULD be *line* comments starting with *three* forward slashes `///` and an optional space.

```typ
/* I'm a regular comment */
#let foo = 1;
// I'm a regular comment.
#let foo = 1;
//// I'm a regular comment.
#let foo = 1;
```

Explanation: There SHOULD be no documentation for `foo` in the three cases. The first comment is not a line comment, the second and the third one don't start with exact three forward slashes. However, the language server will regard them as doc comments loosely.

=== Example 3

The comments SHOULD be consecutively and exactly placed aside the associating definition.

```typ
/// 1
/// 2
#let foo = 1;
```

Explanation: The documentation of `foo` is `"1\\n2"`.

```typ
/// 1

/// 2
#let bar = 1;
```

Explanation: The documentation of `bar` is `"2"`, because there is a space between `/// 1` and `/// 2`.

```typ
/// 1
/// 2

#let baz = 1;
```

Explanation: There SHOULD be no documentation for `baz`, because the comments is not exactly placed before the let statement of the `baz`.

=== Module-Level Docstring

A module-level appears at the beginning of the module (file).

=== Example 4

Given a file `foo.typ` containing code:

```typ
/// 1

/// 2
#let baz = 1;
```

Explanation: The documentation of the module `foo` (`foo.typ`) is `"1"`. It is not `"1\n2"`, because there is a space between `/// 1` and `/// 2`.

=== Example 5

Given a file `foo.typ` containing code:

```typ
// License: Apache 2.0
/// 1
```

Explanation: The documentation of the module `foo` (`foo.typ`) is `"1"`. It is not `"License: Apache 2.0\n1"`, because `// License: Apache 2.0` is not a strict doc comment.

=== Variable Docstring

A variable appears exactly before some let statement (the ast starting with `#let` or `let`). BNF Syntax:

```
VAR_DOCSTRING_CONTENT ::= MARKUP { VAR_SUB_ANNOATATION } [ VAR_INIT_ANNOATATION ]
```

=== Example 6

You can use an arrow `->` following a type annotation to mark the type of the _initializer expression_ of the let statement. The _initializer expression_ is the expression at the right side of the equal marker in the let statement. BNF Syntax:

```
VAR_INIT_ANNOATATION ::= '-> ' TYPE_ANNOATATION
```

```typ
/// -> int
#let f(x) = { /* code */ };
```

Explanation: The docstring tells that the type of `{ /* code */ }` is `int`. Thus, the *resultant type* of the function `f` is also annotated as `int`.

```typ
/// -> float
#let G = { /* code */ };
```

Explanation: The docstring tells that the type of `{ /* code */ }` is `float`. Thus, the *type* of the variable `G` is also annotated as `float`.

=== Example 7

You can use a list item `- name (type): description` to document the related variable at the left side of the let statement. BNF Syntax:

```
VAR_SUB_ANNOATATION ::= '- ' NAME '(' TYPE_ANNOATATION ')' ':' MARKUP
```

```typ
/// - x (int): The input of the function `f`.
#let f(x, y) = { /* code */ };
```

Explanation: The docstring tells that the type of `x` is `int` and the documentation of `x` is "The input of the function `f`."


```typ
/// - x (any): The swapped value from `y`.
#let (x, y) = (y, x);
```

Explanation: The docstring tells that the type of `x` at the left side is `any` and its documentation is "The swapped value from `y`." The variables at the right side of the let statement are not documented by the docstring.

=== Examples in Docstrings

You can use `#example` function to provide examples in docstrings.

=== Example 8

````typ
#example(`
$ sum f(x) = 10 $
`)
````

The docstring tells that there is an associated example in the docstring. It will be rendered as a code block following the rendered result when possible:

#rect(width: 100%)[
  ```typ
  $ sum f(x) = 10 $
  ```
  $ sum f(x) = 10 $
]

=== Type Annotations in Docstrings

A type annotation is a comma separated list containing types. BNF Syntax:

```
TYPE_ANNOATATION ::= TYPE { ',' TYPE }
```

Currently, only built-in types and the generic array type are supported in docstrings.

The list of built-in types:

- `any`
- `content`
- `none`
- `auto`
- `bool` or `boolean`
- `false`
- `true`
- `int` or `integer`
- `float`
- `length`
- `angle`
- `ratio`
- `relative`
- `fraction`
- `str` or `string`
- `color`
- `gradient`
- `pattern`
- `symbol`
- `version`
- `bytes`
- `label`
- `datetime`
- `duration`
- `styles`
- `array`
- `dictionary`
- `function`
- `arguments`
- `type`
- `module`
- `plugin`
