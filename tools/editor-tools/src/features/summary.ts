import van, { ChildDom } from "vanjs-core";
const { div, a, span, code, br } = van.tags;

const DOC_MOCK = {
  fonts: [
    {
      name: "Song Ti",
      postscriptName: "SongTi",
      path: "C:\\Users",
      uses: [
        { span: "", content: "" },
        { span: "", content: "" },
        { span: "", content: "" },
      ],
    },
    {
      name: "Times New Roman",
      postscriptName: "TimesNewRoman",
      path: "C:\\Users",
      uses: [
        { span: "", content: "" },
        { span: "", content: "" },
        { span: "", content: "" },
        { span: "", content: "" },
      ],
    },
    {
      name: "Microsoft YaHei",
      postscriptName: "MicrosoftYaHei",
      path: "C:\\Users",
      uses: [
        { span: "", content: "" },
        { span: "", content: "" },
      ],
    },
  ],
};

interface CompileArgs {
  root: string;
  fontPaths: string[];
  inputs: Record<string, string>;
}

const ARGS_MOCK: CompileArgs = {
  root: "C:\\Users\\OvO\\work\\rust\\tinymist",
  fontPaths: [
    "C:\\Users\\OvO\\work\\rust\\tinymist\\assets\\fonts",
    "C:\\Users\\OvO\\work\\assets\\fonts",
  ],
  inputs: {
    theme: "dark",
    context: '{"preview":true}',
  },
};

export const Summary = () => {
  const docMetrics = van.state(DOC_MOCK);
  const compileArgs = van.state(ARGS_MOCK);

  const FontSlot = (font: any) => {
    return div(
      { style: "margin: 1.2em; margin-left: 0.5em" },
      a({ href: font.path, style: "font-size: 1.2em" }, font.name),
      " has ",
      a({ href: "javascript:void(0)" }, font.uses.length),
      " use(s).",
      br(),
      code("PostScriptName"),
      ": ",
      code(font.postscriptName)
    );
  };

  const ArgSlots = () => {
    const res: ChildDom[] = [];
    let val = compileArgs.val;
    if (val.root) {
      res.push(
        div(
          a({ href: "javascript:void(0)" }, code("root")),
          ": ",
          code(val.root)
        )
      );
    }

    for (let i = 0; i < val.fontPaths.length; i++) {
      res.push(
        div(
          a({ href: "javascript:void(0)" }, code(`font-path(${i})`)),
          ": ",
          code(val.fontPaths[i])
        )
      );
    }

    if (val.inputs) {
      const codeList: ChildDom[] = [];
      for (const key of Object.keys(val.inputs)) {
        codeList.push(
          span({ style: "color: #DEC76E" }, key),
          span({ style: "color: #7DCFFF" }, "="),
          val.inputs[key]
        );
      }

      res.push(
        div(
          a({ href: "javascript:void(0)" }, code("sys.inputs")),
          ": ",
          code(...codeList)
        )
      );
    }

    return res;
  };

  return div(
    {
      class: "flex-col",
      style: "justify-content: center; align-items: center; gap: 10px;",
    },
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      div(
        van.derive(() => `This document is compiled with following arguments.`)
      ),
      div({ style: "margin: 1.2em; margin-left: 0.5em" }, ...ArgSlots())
    ),
    div(
      { class: `tinymist-card`, style: "flex: 1; width: 100%; padding: 10px" },
      div(
        van.derive(
          () => `This document uses ${docMetrics.val.fonts.length} fonts.`
        )
      ),
      (_dom?: Element) => div(...docMetrics.val.fonts.map(FontSlot))
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Tinymist service.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(
          `Its version is `,
          a({ href: "javascript:void(0)" }, "0.11.2"),
          `.`
        ),
        div(`It is compiled with optimization level `, "3", `.`),
        div(
          `It is connecting to the client `,
          code({ style: "font-style: italic" }, "VSCode 1.87.2"),
          `.`
        )
      )
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Typst compiler.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(
          `Its version is `,
          a({ href: "javascript:void(0)" }, "0.11.0"),
          `.`
        ),
        div(
          `It identifies `,
          a({ href: "javascript:void(0)" }, "374"),
          ` font variants.`
        )
      )
    ),
    div(
      {
        class: `tinymist-card hidden`,
        style: "flex: 1; width: 100%; padding: 10px",
      },
      div(`The Typst formatters.`),
      div(
        { style: "margin: 0.8em; margin-left: 0.5em" },
        div(`It uses typstyle with following configurations.`),
        code(
          { style: "margin-left: 0.5em" },
          a({ href: "javascript:void(0)", style: "color: #DEC76E" }, "columns"),
          span({ style: "color: #7DCFFF" }, "="),
          "120"
        ),
        div(
          `The version of typstyle is `,
          a({ href: "javascript:void(0)" }, "0.11.7"),
          `.`
        ),
        div(
          `The version of typstfmt is `,
          a({ href: "javascript:void(0)" }, "0.2.9"),
          `.`
        )
      )
    )
  );
};
