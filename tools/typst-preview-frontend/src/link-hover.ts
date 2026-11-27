export type LinkDescriptions = Readonly<Record<string, readonly string[]>>;

// The HTML preview serializes this function into its replacement document.
export function addLinkTitles(root: ParentNode, descriptions: LinkDescriptions = {}) {
  const svgNamespace = "http://www.w3.org/2000/svg";
  for (const anchor of root.querySelectorAll("a")) {
    const href = anchor.getAttribute("href") ?? anchor.getAttribute("xlink:href");
    if (href === null || anchor.hasAttribute("title")) {
      continue;
    }

    const doc = anchor.ownerDocument;
    let title = href || doc.URL;
    if (href.startsWith("#")) {
      const location = anchor
        .getAttribute("onclick")
        ?.match(/^handleTypstLocation\(this, (\d+), ([^,]+), ([^)]+)\); return false$/);
      if (location) {
        const [, page, x, y] = location;
        const details = descriptions[`${page}, ${x}, ${y}`] ?? [];
        title = [`Page ${page}`, ...details].join("\n");
      } else {
        let id = href.slice(1);
        try {
          id = decodeURIComponent(id);
        } catch {
          // A malformed escape may be part of the literal ID.
        }
        const target = doc.getElementById(id);
        if (target) {
          const source =
            target.tagName.toLowerCase() === "figure"
              ? (target.querySelector(":scope > figcaption") ?? target)
              : target;
          let rawText = "";
          const walk = (node: Node) => {
            if (node.nodeType === 3) {
              rawText += node.nodeValue ?? "";
              return;
            }
            if (node.nodeType !== 1) {
              return;
            }
            const tag = (node as Element).tagName.toLowerCase();
            if (tag === "style" || tag === "script") {
              return;
            }
            for (const child of node.childNodes) {
              walk(child);
            }
          };
          walk(source);
          const body = rawText.replace(/\s+/g, " ").trim();
          if (body) {
            title = `${href}\n${body}`;
          }
        }
      }
    }

    if (anchor.namespaceURI === svgNamespace) {
      const previous = anchor.querySelector(":scope > title");
      if (previous && !previous.hasAttribute("data-tinymist-link-hover")) {
        continue;
      }
      const element = previous ?? doc.createElementNS(svgNamespace, "title");
      if (element.textContent !== title) {
        element.textContent = title;
      }
      if (!previous) {
        element.setAttribute("data-tinymist-link-hover", "");
        anchor.prepend(element);
      }
    } else {
      anchor.setAttribute("title", title);
    }
  }
}

let pagedLinkDescriptions: LinkDescriptions = {};

export function setPagedLinkDescriptions(descriptions: LinkDescriptions) {
  pagedLinkDescriptions = descriptions;
}

export function addPagedLinkTitles(root: SVGElement) {
  addLinkTitles(root, pagedLinkDescriptions);
}
