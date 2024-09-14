import "./docs.css";
import van, { State, ChildDom } from "vanjs-core";
const { div, h1, h2, h3, code, a, p, i, span, strong } = van.tags;

// import { docsMock } from "./docs.mock";
const docsMock = "";

export const Docs = () => {
  const parsedDocs: State<DocElement> = van.state({
    contents: [],
    children: [],
    kind: DocKind.None,
    id: "",
    data: null,
  } as DocElement);

  const favoritePlaceholders = `:[[preview:DocContent]]:`;
  van.derive(async () => {
    const inp = favoritePlaceholders.startsWith(":")
      ? docsMock
      : decodeURIComponent(atob(favoritePlaceholders));
    if (!inp) {
      return;
    }

    parsedDocs.val = await recoverDocsStructure(inp);
  });

  return div(
    {
      class: "tinymist-docs flex-col",
      style: "justify-content: center; align-items: center; gap: 10px;",
    },
    div(
      {
        style: "flex: 1; width: 100%; padding: 10px",
      },
      (_dom?: Element) => {
        const v = parsedDocs.val;
        console.log("updated", v);
        return div(MakeDoc(v));
      }
    )
  );
};

const enum TokenKind {
  Text,
  PackageStart,
  PackageEnd,
  ParamDocStart,
  ParamDocEnd,
  ErrorStart,
  ErrorEnd,
  ModuleStart,
  ModuleEnd,
  SymbolStart,
  SymbolEnd,
  SigStart,
  SigEnd,
  ParamStart,
  ParamEnd,
  Comment,
}

const enum DocKind {
  None,
  Package,
  Module,
  Symbol,
  Param,
  SigOrParam,
}

interface DocElement {
  contents: string[];
  children: DocElement[];
  kind: DocKind;
  id: string;
  data: any;
}

async function recoverDocsStructure(content: string) {
  console.log("recoverDocsStructure", { content });
  // split content by comment
  let reg = /<!--(.*?)-->/g;
  let tokenPromises = [];
  let match;
  let lastIndex = 0;
  while ((match = reg.exec(content))) {
    tokenPromises.push(
      Promise.resolve([TokenKind.Text, content.slice(lastIndex, match.index)])
    );
    tokenPromises.push(identifyCommentToken(match[1]));
    lastIndex = reg.lastIndex;
  }

  tokenPromises.push(Promise.resolve(content.slice(lastIndex)));
  const tokens = await Promise.all(tokenPromises);

  let packageStack = [];
  let structStack = [];
  let current = {
    contents: [],
    children: [],
    kind: DocKind.None,
    id: "",
    data: {},
  } as DocElement;
  let currentPkg = current;

  for (const token of tokens) {
    switch (token[0]) {
      case TokenKind.PackageStart:
        structStack.push(current);
        packageStack.push(currentPkg);
        current = {
          contents: [],
          children: [],
          kind: DocKind.Package,
          id: "",
          data: token[1],
        };
        currentPkg = current;
        break;
      case TokenKind.PackageEnd:
        const pkg = current;
        current = structStack.pop()!;
        currentPkg = packageStack.pop()!;
        current.children.push(pkg);
        break;
      case TokenKind.ErrorStart:
        currentPkg.data.error = token[1];
        break;
      case TokenKind.ErrorEnd:
        break;
      case TokenKind.ParamDocStart: {
        structStack.push(current);
        let sym = undefined;
        for (let i = structStack.length - 1; i >= 0; i--) {
          if (structStack[i].kind === DocKind.Symbol) {
            sym = structStack[i];
            break;
          }
        }
        current = {
          contents: [],
          children: [],
          kind: DocKind.Param,
          id: token[1],
          data: {
            name: token[1],
          },
        };
        if (sym) {
          current.id = `${sym.id}-param-${token[1]}`;
          const renderedParams = (sym.data.renderedParams =
            sym.data.renderedParams || {});
          renderedParams[current.id] = current;
        }
        break;
      }
      case TokenKind.ParamDocEnd: {
        current = structStack.pop()!;
        break;
      }
      case TokenKind.ModuleStart:
        structStack.push(current);
        current = {
          contents: [],
          children: [],
          kind: DocKind.Module,
          id: token[1],
          data: token[2],
        };
        break;
      case TokenKind.ModuleEnd:
        const module = current;
        current = structStack.pop()!;
        current.children.push(module);
        break;
      case TokenKind.SymbolStart:
        structStack.push(current);
        current = {
          contents: [],
          children: [],
          kind: DocKind.Symbol,
          id: token[1],
          data: token[2],
        };
        break;
      case TokenKind.SymbolEnd:
        const symbol = current;
        current = structStack.pop()!;
        current.children.push(symbol);
        break;
      case TokenKind.SigStart:
      case TokenKind.ParamStart:
        structStack.push(current);
        current = {
          contents: [],
          children: [],
          kind: DocKind.SigOrParam,
          id: "",
          data: {},
        };
        break;
      case TokenKind.SigEnd:
      case TokenKind.ParamEnd:
        current = structStack.pop()!;
        break;
      case TokenKind.Comment:
        console.log("Comment", token[1]);
        break;
      case TokenKind.Text:
        current.contents.push(token[1]);
        break;
    }
  }

  return current;
}

async function identifyCommentToken(comment: string) {
  const cs = comment.trim().split(" ");
  switch (cs[0]) {
    case "begin:package":
      return [TokenKind.PackageStart, JSON.parse(await base64ToUtf8(cs[1]))];
    case "end:package":
      return [TokenKind.PackageEnd, JSON.parse(await base64ToUtf8(cs[1]))];
    case "begin:param-doc":
      return [TokenKind.ParamDocStart, cs[1]];
    case "end:param-doc":
      return [TokenKind.ParamDocEnd, cs[1]];
    case "begin:errors":
      return [TokenKind.ErrorStart, JSON.parse(await base64ToUtf8(cs[1]))];
    case "end:errors":
      return [TokenKind.ErrorEnd, cs[1]];
    case "begin:module":
      return [
        TokenKind.ModuleStart,
        cs[1],
        JSON.parse(await base64ToUtf8(cs[2])),
      ];
    case "end:module":
      return [TokenKind.ModuleEnd, cs[1]];
    case "begin:symbol":
      return [
        TokenKind.SymbolStart,
        cs[1],
        JSON.parse(await base64ToUtf8(cs[2])),
      ];
    case "end:symbol":
      return [TokenKind.SymbolEnd, cs[1]];
    case "begin:sig":
      return [TokenKind.SigStart, cs[1]];
    case "end:sig":
      return [TokenKind.SigEnd, cs[1]];
    case "begin:param":
      return [TokenKind.ParamStart, cs[1]];
    case "end:param":
      return [TokenKind.ParamEnd, cs[1]];
    case "typlite:begin:list-item":
    case "typlite:end:list-item":
      return [TokenKind.Text, ""];
    default:
      return [TokenKind.Comment, comment];
  }
}

async function base64ToUtf8(base64: string) {
  const base64Url = `data:text/plain;base64,${base64}`;
  const res = await fetch(base64Url);
  return await res.text();
}

function getKnownModules(v: DocElement, s: Set<string>) {
  for (const child of v.children) {
    if (child.kind === DocKind.Module) {
      s.add(child.id);
    }
    getKnownModules(child, s);
  }
}

function MakeDoc(v: DocElement) {
  const knownModules = new Set<string>();
  getKnownModules(v, knownModules);
  console.log("MakeDoc", v, knownModules);

  function Item(v: DocElement): ChildDom {
    switch (v.kind) {
      case DocKind.Package:
        return PackageItem(v);
      case DocKind.Module:
        if (!v.data.prefix) {
          return ModuleBody(v);
        }
        return ModuleItem(v);
      case DocKind.Symbol:
        const kind = v.data.kind;

        switch (kind) {
          case "function":
            return FuncItem(v);
          case "constant":
            return ConstItem(v);
          case "module":
            return ModuleRefItem(v);
          default:
            return div();
        }
      case DocKind.None:
        return div(...v.children.map(Item));
      default:
        return div();
    }
  }

  function ModuleBody(v: DocElement) {
    const modules = [];
    const functions = [];
    const constants = [];
    const unknowns = [];
    for (const child of v.children) {
      switch (child.kind) {
        case DocKind.Module:
          modules.push(child);
          break;
        case DocKind.Symbol:
          switch (child.data.kind) {
            case "function":
              functions.push(child);
              break;
            case "constant":
              constants.push(child);
              break;
            case "module":
              modules.push(child);
              break;
            default:
              unknowns.push(child);
              break;
          }
          break;
      }
    }

    const chs = [];

    if (modules.length > 0) {
      chs.push(h2("Modules"), div(...modules.map(ModuleRefItem)));
    }

    if (constants.length > 0) {
      chs.push(h2("Constants"), div(...constants.map(Item)));
    }

    if (functions.length > 0) {
      chs.push(h2("Functions"), div(...functions.map(Item)));
    }

    if (unknowns.length > 0) {
      chs.push(h2("Unknowns"), div(...unknowns.map(Item)));
    }

    return div(...chs);
  }

  function ModuleItem(v: DocElement) {
    return div(
      { class: "tinymist-module" },
      h1({ id: `module-${v.id}` }, `Module: ${v.data.prefix}`),
      ModuleBody(v)
    );
  }

  function PackageItem(v: DocElement) {
    console.log("PackageItem", v);
    return div(
      h1(`@${v.data.namespace}/${v.data.name}:${v.data.version}`),
      p(
        span(
          "This documentation is generated locally. Please submit issues to "
        ),
        a(
          { href: "https://github.com/Myriad-Dreamin/tinymist/issues" },
          "tinymist"
        ),
        span(" if you see "),
        strong(i("incorrect")),
        span(" information in it.")
      ),
      // ModuleBody(v)
      ...v.children.map(Item)
    );
  }

  function ModuleRefItem(v: DocElement) {
    const isExternal = !knownModules.has(v.id);

    let body;
    if (isExternal) {
      body = code("external ", v.data.name);
    } else {
      body = code(
        a(
          {
            href: `#module-${v.id}`,
          },
          v.data.name
        )
      );
    }

    return div(
      {
        class: "tinymist-module-ref",
      },
      div(
        {
          class: `detail-header doc-symbol-${v.data.kind}`,

          //   <a href="https://github.com/elixir-lang/elixir/blob/v1.17.2/lib/elixir/lib/float.ex#L283" class="icon-action" rel="help" title="View Source">
          //   <i class="ri-code-s-slash-line" aria-hidden="true"></i>
          //   <span class="sr-only">View Source</span>
          // </a>
        },
        h3({ class: "doc-symbol-name" }, body)
      )
    );
  }

  interface DocParam {
    name: string;
    cano_type: [string, string];
    expr?: string;
  }

  function FuncItem(v: DocElement) {
    const sig = v.data.signature;
    let funcTitle = [code(v.data.name), "("];
    if (sig) {
      // funcTitle.push(...sig.pos.map((e: DocParam) => code(e.name)));
      for (let i = 0; i < sig.pos.length; i++) {
        if (i > 0) {
          funcTitle.push(code(", "));
        }
        funcTitle.push(code(sig.pos[i].name));
      }

      if (sig.rest || Object.keys(sig.named).length > 0) {
        if (sig.pos.length > 0) {
          funcTitle.push(code(", "));
        }
        funcTitle.push(code(".."));
      }
    }
    funcTitle.push(code(")"));
    if (v.data.parsed_docs?.return_ty) {
      funcTitle.push(code(" -> "));
      typeHighlighted(v.data.parsed_docs.return_ty, funcTitle);
    }

    return div(
      {
        class: "tinymist-symbol",
      },
      div(
        {
          class: `detail-header doc-symbol-${v.data.kind}`,
        },
        h3({ class: "doc-symbol-name" }, code(...funcTitle))
      ),
      ...SigPreview(v),
      div({
        style: "margin-left: 0.62em",
        innerHTML: v.contents.join(""),
      }),
      ...SigDocs(v)
    );
  }

  interface DocSignature {
    pos: DocParam[];
    rest: DocParam;
    named: Record<string, DocParam>;
    ret_ty?: [string, string];
    // return;
  }

  function SigDocs(v: DocElement): ChildDom[] {
    const sig: DocSignature = v.data.signature;
    const parsed_docs = v.data.parsed_docs;
    const res: ChildDom[] = [];

    if (!sig) {
      return res;
    }

    const docsMapping = new Map<string, any>();
    // for (const doc of parsed_docs) {
    //   docsMapping.set(doc.name, doc.contents.join(""));
    // }
    // return_ty
    if (parsed_docs?.params) {
      for (const param of parsed_docs.params) {
        docsMapping.set(param.name, param);
      }
    }
    if (v.data.renderedParams) {
      for (const p of Object.values(v.data.renderedParams)) {
        const param = p as DocElement;
        const docs = param.contents.join("");
        const prev = docsMapping.get(param.data.name) || {};
        prev.docs = docs;
        docsMapping.set(param.data.name, prev);
      }
    }
    interface TaggedParam {
      kind: string;
      param: DocParam;
    }

    const paramsAll: TaggedParam[] = [
      ...sig.pos.map((param: DocParam) => ({ kind: "pos", param })),
      ...(sig.rest ? [{ kind: "rest", param: sig.rest }] : []),
      ...Object.entries(sig.named).map(([name, param]) => ({
        kind: "named",
        name,
        param,
      })),
    ];

    if (parsed_docs?.return_ty || sig.ret_ty) {
      let paramTitle = [codeHl("op", "-> ")];
      sigTypeHighlighted(parsed_docs.return_ty, sig.ret_ty, paramTitle);

      res.push(h3("Resultant"));
      res.push(
        div(
          {
            style: "margin-left: 0.62em",
          },
          div(
            {
              style: "margin-left: 0.62em",
            },
            div(
              {
                class: "doc-param-title",
              },
              strong(paramTitle)
            )
          )
        )
      );
    }

    if (paramsAll.length) {
      res.push(h3("Parameters"));
    }

    console.log("SigDocs", { paramsAll, docsMapping });

    for (const { kind, param } of paramsAll) {
      let docs: string[] = [];
      const docsMeta = docsMapping.get(param.name);
      if (docsMeta?.docs) {
        docs = [docsMeta.docs];
      }

      let paramTitle = [
        code(
          {
            id: `param-${v.id}-${param.name}`,
          },
          param.name
        ),
      ];
      if (docsMeta?.types || param.cano_type) {
        paramTitle.push(code(": "));
        // paramTitle += `: ${docsMeta.types}`;
        sigTypeHighlighted(docsMeta?.types, param.cano_type, paramTitle);
      }

      if (param.expr) {
        paramTitle.push(codeHl("op", " = "));
        paramTitle.push(code(param.expr));
      }

      if (kind == "pos") {
        paramTitle.push(code(" (positional)"));
      } else if (kind == "rest") {
        paramTitle.push(code(" (rest)"));
      }

      const docsAll = docs.join("");

      res.push(
        div(
          {
            style: "margin-left: 0.62em",
          },
          div(
            {
              class: "doc-param-title",
            },
            strong(code(paramTitle))
          ),
          div({
            style: "margin-left: 0.62em",
            innerHTML: docsAll ? docsAll : "<p>-</p>",
          })
        )
      );
    }

    return res;
  }

  function SigPreview(v: DocElement): ChildDom[] {
    const sig = v.data.signature;
    if (!sig) {
      return [];
    }

    const res: ChildDom[] = [];
    const paramsAll = [
      ...sig.pos.map((param: DocParam) => ({ kind: "pos", param })),
      ...Object.entries(sig.named).map(([name, param]) => ({
        kind: "named",
        name,
        param,
      })),
      ...(sig.rest ? [{ kind: "rest", param: sig.rest }] : []),
    ];
    // ...paramsAll.map(({ kind, param }, i) => {
    //   if (i > 0) {
    //     return code(", ");
    //   }
    //   return code(param.name);
    // }),

    const sigTitle = [
      code(kwHl("let")),
      code(" "),
      code(fnHl(v.data.name)),
      code("("),
    ];
    for (let i = 0; i < paramsAll.length; i++) {
      if (i > 0) {
        sigTitle.push(code(", "));
      }
      let paramTitle = [];
      if (paramsAll[i].kind == "rest") {
        paramTitle.push(code(".."));
      }
      paramTitle.push(code(paramsAll[i].param.name));
      if (paramsAll[i].kind == "named") {
        paramTitle.push(code("?"));
      }
      sigTitle.push(
        a(
          {
            href: `#param-${v.id}-${paramsAll[i].param.name}`,
          },
          ...paramTitle
        )
      );
    }
    sigTitle.push(code(")"));
    if (v.data.parsed_docs?.return_ty) {
      sigTitle.push(code(" -> "));
      typeHighlighted(v.data.parsed_docs.return_ty, sigTitle);
    }
    sigTitle.push(code(";"));

    res.push(
      div(
        { style: "margin-left: 0.62em" },
        div({
          style: "font-size: 1.5em; margin: 0.5em 0",
        }),
        div(
          {
            style: "margin: 0 1em",
          },
          code(...sigTitle)
        )
      )
    );

    return res;
  }

  function ConstItem(v: DocElement) {
    return div(
      {
        class: "tinymist-symbol",
      },
      div(
        {
          class: `detail-header doc-symbol-${v.data.kind}`,
        },
        h3(
          { class: "doc-symbol-name" },
          code(`${v.data.name}`)
          // code(
          //   {
          //     style: "float: right; line-height: 1em",
          //   },
          //   `${v.data.kind}`
          // )
        )
      ),
      div({
        style: "margin-left: 0.62em",
        innerHTML: v.contents.join(""),
      })
    );
  }

  return Item(v);
}

function sigTypeHighlighted(
  types: string | undefined,
  inferred: [string, string] | undefined,
  target: ChildDom[]
) {
  console.log("sigTypeHighlighted", { types, inferred });
  if (types) {
    typeHighlighted(types, target);
  } else if (inferred) {
    const rendered: ChildDom[] = [];
    typeHighlighted(inferred[0], rendered, "|");
    const infer = span(
      { class: "code-kw type-inferred", title: "inferred by type checker" },
      "infer"
    );
    target.push(
      code(
        { class: "type-inferred" },
        infer,
        code(" "),
        span({ class: "type-inferred-as", title: inferred[1] }, ...rendered)
      )
    );
  }
}

function typeHighlighted(
  types: string,
  target: ChildDom[],
  by: RegExp | string = /[|,]/g
) {
  const type = types.split(by);
  for (let i = 0; i < type.length; i++) {
    if (i > 0) {
      target.push(code(" | "));
    }
    const ty = type[i].trim();
    switch (ty) {
      case "int":
      case "integer":
        target.push(code({ class: "type-int" }, ty));
        break;
      case "float":
        target.push(code({ class: "type-float" }, ty));
        break;
      case "string":
      case "array":
      case "dictionary":
      case "content":
      case "str":
      case "bool":
      case "boolean":
        target.push(code({ class: "type-builtin" }, ty));
        break;
      case "auto":
        target.push(code({ class: "type-auto" }, ty));
        break;
      case "none":
        target.push(code({ class: "type-none" }, ty));
        break;
      default:
        target.push(code(type[i]));
        break;
    }
  }
}

function kwHl(kw: string) {
  return code({ class: "code-kw" }, kw);
}

function fnHl(fn: string) {
  return code({ class: "code-func" }, fn);
}

function codeHl(cls: string, c: string) {
  return code({ class: `code-${cls}` }, c);
}
