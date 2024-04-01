import van from "vanjs-core";
const { div, a, br, span, code } = van.tags;

const DIAG_MOCK = {};

export const Diagnostics = () => {
  const diagnostics = van.state(DIAG_MOCK);
  void diagnostics;

  return div(
    {
      class: "flex-col",
      style: "justify-content: center; align-items: center; gap: 10px;",
    },
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      code(
        { width: "100%" },
        `cannot add integer and alignment`,
        br(),
        a({ href: "javascript:void(0)" }, "test.typ(3, 19)"),
        `: error occurred in this call of function \`f\``,
        br(),
        a({ href: "javascript:void(0)" }, `test.typ(6, 2)`),
        `: error occurred in this call of function \`g\``
      )
    ),
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      "Expression explained.",
      div(
        {
          style: "margin: 0.8em 0",
        },
        "The expression is: ",
        code(a({ href: "javascript:void(0)" }, `x`)),
        code(` + `),
        code(a({ href: "javascript:void(0)" }, `y`)),
        ", at location: test.typ(2, 16):",
        br(),
        code(
          {
            style: "margin: 0.5em",
          },
          `#let f(x, y)  = `,
          span({ style: "text-decoration: underline" }, `x + y`),
          `;`
        ),
        br(),
        "where ",
        code(a({ href: "javascript:void(0)" }, `x`)),
        " is the ",
        "1st",
        " function parameter of ",
        code(a({ href: "javascript:void(0)" }, `f`)),
        ".",
        br(),
        "where ",
        code(a({ href: "javascript:void(0)" }, `y`)),
        " is the ",
        "2nd",
        " function parameter of ",
        code(a({ href: "javascript:void(0)" }, `f`)),
        "."
      ),
      div(
        {
          style: "margin: 0.8em 0",
        },
        code(a({ href: "javascript:void(0)" }, `f`)),
        " is called with arguments ",
        code("f("),
        code(a({ href: "javascript:void(0)" }, `x`)),
        code(` = 1, `),
        code(a({ href: "javascript:void(0)" }, `y`)),
        code(" = left)"),
        ", at location: test.typ(3, 19):",
        br(),
        code(
          {
            style: "margin: 0.5em",
          },
          `#let g(x, y, z) = `,
          span({ style: "text-decoration: underline" }, `f(x, y)`),
          ` + z;`
        ),
        br(),
        "where ",
        code(a({ href: "javascript:void(0)" }, `x`)),
        " is the ",
        "1st",
        " function parameter of ",
        code(a({ href: "javascript:void(0)" }, `g`)),
        ".",
        br(),
        "where ",
        code(a({ href: "javascript:void(0)" }, `y`)),
        " is the ",
        "2nd",
        " function parameter of ",
        code(a({ href: "javascript:void(0)" }, `g`)),
        "."
      ),
      div(
        {
          style: "margin: 0.8em 0",
        },
        code(a({ href: "javascript:void(0)" }, `g`)),
        " is called with arguments ",
        code("g("),
        code(`x`),
        code(` = 1, `),
        code(`y`),
        code(" = left, z = red)"),
        ", at location: test.typ(6, 2):",
        br(),
        code(
          {
            style: "margin: 0.5em",
          },
          `#`,
          span({ style: "text-decoration: underline" }, `g(1, left, red)`),
          `;`
        )
      )
    )
  );
};
