//
// div(
//     cls := "typst-editable-div title",
//     contentEditable := true,
//     styleAttr(
//       "padding: 0.2em 0.1em; text-align: center; font-size: 42px; font-weight: 700; font-family: 'Libertinus serif', sans-serif",
//     ),
//     "Safe Sandboxing WebAssembly Programs via Formal Verification",
//   ),

export class WysiwygDocument {
  constructor(elem: HTMLElement) {
    {
      const div = document.createElement("div");
      div.className = "typst-editable-div title";
      div.contentEditable = "true";
      div.style.padding = "0.2em 0.1em";
      div.style.textAlign = "center";
      div.style.fontSize = "42px";
      div.style.fontWeight = "700";
      div.style.fontFamily = "'Libertinus serif', sans-serif";
      div.innerText = "Safe Sandboxing WebAssembly Programs via Formal Verification";
      elem.appendChild(div);
    }
  }

  reset() {}
  dispose() {}
}
