import { describe, expect, it } from "vitest";
import { htmlPreviewClientScript } from "./html-preview";
import { addLinkTitles } from "./link-hover";

const SVG_NS = "http://www.w3.org/2000/svg";
const HTML_NS = "http://www.w3.org/1999/xhtml";

class MockText {
  readonly nodeType = 3;
  constructor(public nodeValue: string) {}
}

class MockElement {
  readonly nodeType = 1;
  readonly attrs = new Map<string, string>();
  readonly childNodes: Array<MockElement | MockText> = [];
  setAttributeCount = new Map<string, number>();

  constructor(
    public readonly ownerDocument: MockDocument,
    public readonly namespaceURI: string,
    public readonly tagName: string,
    attrs: Record<string, string> = {},
    children: Array<MockElement | MockText | string> = [],
  ) {
    for (const [key, value] of Object.entries(attrs)) {
      this.attrs.set(key, value);
    }
    for (const child of children) {
      this.childNodes.push(typeof child === "string" ? new MockText(child) : child);
    }
  }

  get textContent(): string {
    return this.childNodes
      .map((child) => (child instanceof MockText ? child.nodeValue : child.textContent))
      .join("");
  }

  set textContent(value: string) {
    this.childNodes.splice(0, this.childNodes.length, new MockText(value));
  }

  getAttribute(name: string): string | null {
    return this.attrs.has(name) ? this.attrs.get(name)! : null;
  }

  hasAttribute(name: string): boolean {
    return this.attrs.has(name);
  }

  setAttribute(name: string, value: string): void {
    this.attrs.set(name, value);
    this.setAttributeCount.set(name, (this.setAttributeCount.get(name) ?? 0) + 1);
  }

  prepend(node: MockElement): void {
    this.childNodes.unshift(node);
  }

  querySelector(selector: string): MockElement | null {
    if (selector.startsWith(":scope > ")) {
      const targetTag = selector.slice(":scope > ".length).toLowerCase();
      for (const child of this.childNodes) {
        if (child instanceof MockElement && child.tagName.toLowerCase() === targetTag) {
          return child;
        }
      }
      return null;
    }
    return this.querySelectorAll(selector)[0] ?? null;
  }

  querySelectorAll(selector: string): MockElement[] {
    const targetTag = selector.toLowerCase();
    const result: MockElement[] = [];
    const visit = (node: MockElement) => {
      for (const child of node.childNodes) {
        if (child instanceof MockElement) {
          if (child.tagName.toLowerCase() === targetTag) {
            result.push(child);
          }
          visit(child);
        }
      }
    };
    visit(this);
    return result;
  }
}

class MockDocument {
  readonly URL = "http://127.0.0.1:23625/";
  readonly root: MockElement;

  constructor(children: Array<MockElement | MockText | string> = []) {
    this.root = new MockElement(this, HTML_NS, "html", {}, children);
  }

  el(
    tagName: string,
    attrs: Record<string, string> = {},
    children: Array<MockElement | MockText | string> = [],
    namespaceURI = HTML_NS,
  ): MockElement {
    return new MockElement(this, namespaceURI, tagName, attrs, children);
  }

  createElementNS(namespaceURI: string, tagName: string): MockElement {
    return new MockElement(this, namespaceURI, tagName);
  }

  querySelectorAll(selector: string): MockElement[] {
    return this.root.querySelectorAll(selector);
  }

  getElementById(id: string): MockElement | null {
    const visit = (node: MockElement): MockElement | null => {
      if (node.getAttribute("id") === id) {
        return node;
      }
      for (const child of node.childNodes) {
        if (child instanceof MockElement) {
          const found = visit(child);
          if (found) {
            return found;
          }
        }
      }
      return null;
    };
    return visit(this.root);
  }
}

describe("addLinkTitles", () => {
  it("adds and incrementally updates SVG link tooltips while preserving author titles", () => {
    const doc = new MockDocument();
    const external = doc.el("a", { "xlink:href": "https://example.com" }, [], SVG_NS);
    const mailto = doc.el("a", { "xlink:href": "mailto:test@example.com" }, [], SVG_NS);
    const tel = doc.el("a", { "xlink:href": "tel:+123456789" }, [], SVG_NS);
    const internal = doc.el(
      "a",
      {
        "xlink:href": "#",
        onclick: "handleTypstLocation(this, 2, 70.5, 140.25); return false",
      },
      [],
      SVG_NS,
    );
    const customTitle = doc.el("title", {}, ["Author title"], SVG_NS);
    const authored = doc.el(
      "a",
      { "xlink:href": "https://authored.example" },
      [customTitle],
      SVG_NS,
    );
    const svg = doc.el("svg", {}, [external, mailto, tel, internal, authored], SVG_NS);
    doc.root.childNodes.push(svg);

    addLinkTitles(svg as unknown as ParentNode, {
      "2, 70.5, 140.25": ["Old heading"],
    });

    expect(external.querySelector(":scope > title")?.textContent).toBe("https://example.com");
    expect(mailto.querySelector(":scope > title")?.textContent).toBe("mailto:test@example.com");
    expect(tel.querySelector(":scope > title")?.textContent).toBe("tel:+123456789");
    expect(internal.querySelector(":scope > title")?.textContent).toBe("Page 2\nOld heading");
    expect(authored.querySelector(":scope > title")?.textContent).toBe("Author title");

    const generatedTitle = internal.querySelector(":scope > title")!;
    expect(generatedTitle.setAttributeCount.get("data-tinymist-link-hover")).toBe(1);

    // Incremental update refreshes the existing generated <title> without re-setting its marker attribute.
    addLinkTitles(svg as unknown as ParentNode, {
      "2, 70.5, 140.25": ["New heading"],
    });

    expect(internal.querySelector(":scope > title")).toBe(generatedTitle);
    expect(generatedTitle.textContent).toBe("Page 2\nNew heading");
    expect(generatedTitle.setAttributeCount.get("data-tinymist-link-hover")).toBe(1);
  });

  it("resolves HTML fragments, prefers figure captions, skips style/script text, and honors title", () => {
    const doc = new MockDocument();
    const heading = doc.el("h2", { id: "sec 1" }, ["1. Intro"]);
    const figureWithCaption = doc.el("figure", { id: "fig-table" }, [
      doc.el("table", {}, [doc.el("tr", {}, [doc.el("td", {}, ["Huge table cell"])])]),
      doc.el("svg", {}, [doc.el("style", {}, [".cls { fill: red; }"], SVG_NS)], SVG_NS),
      doc.el("figcaption", {}, ["Figure 1: Results summary"]),
    ]);
    const figureWithoutCaption = doc.el("figure", { id: "fig-svg" }, [
      doc.el(
        "svg",
        {},
        [
          doc.el("style", {}, [".cls { fill: blue; }"], SVG_NS),
          doc.el("text", {}, ["Diagram label"], SVG_NS),
        ],
        SVG_NS,
      ),
    ]);
    const bibEntry = doc.el("li", { id: "bib-1" }, [
      doc.el("span", { class: "prefix" }, ["[1]"]),
      " Ada Example, First Work, 2024.",
    ]);

    const linkHeading = doc.el("a", { href: "#sec%201" }, ["Sec 1"]);
    const linkCaption = doc.el("a", { href: "#fig-table" }, ["Fig 1"]);
    const linkSvgFigure = doc.el("a", { href: "#fig-svg" }, ["Fig 2"]);
    const linkBib = doc.el("a", { href: "#bib-1" }, ["[1]"]);
    const linkAuthored = doc.el("a", { href: "#sec%201", title: "Custom" }, ["Keep"]);
    const linkExternal = doc.el("a", { href: "mailto:hi@example.com" }, ["Mail"]);

    doc.root.childNodes.push(
      heading,
      figureWithCaption,
      figureWithoutCaption,
      bibEntry,
      linkHeading,
      linkCaption,
      linkSvgFigure,
      linkBib,
      linkAuthored,
      linkExternal,
    );

    addLinkTitles(doc as unknown as ParentNode);

    expect(linkHeading.getAttribute("title")).toBe("#sec%201\n1. Intro");
    expect(linkCaption.getAttribute("title")).toBe("#fig-table\nFigure 1: Results summary");
    expect(linkSvgFigure.getAttribute("title")).toBe("#fig-svg\nDiagram label");
    expect(linkBib.getAttribute("title")).toBe("#bib-1\n[1] Ada Example, First Work, 2024.");
    expect(linkAuthored.getAttribute("title")).toBe("Custom");
    expect(linkExternal.getAttribute("title")).toBe("mailto:hi@example.com");
  });

  it("serializes addLinkTitles without external closures", () => {
    const isolated = new Function(
      `return (${addLinkTitles.toString()});`,
    )() as typeof addLinkTitles;
    const doc = new MockDocument();
    const target = doc.el("h2", { id: "dest" }, ["Destination"]);
    const link = doc.el("a", { href: "#dest" }, ["Go"]);
    doc.root.childNodes.push(target, link);

    isolated(doc as unknown as ParentNode);
    expect(link.getAttribute("title")).toBe("#dest\nDestination");
  });
});

describe("htmlPreviewClientScript", () => {
  it("requests current on open, skips identical html, and reapplies link titles on updates", () => {
    const sockets: Array<{
      url: string;
      binaryType: string;
      sent: string[];
      onopen?: () => void;
      onmessage?: (event: { data: ArrayBuffer }) => void;
      onclose?: () => void;
      send: (msg: string) => void;
    }> = [];

    class FakeWebSocket {
      binaryType = "";
      sent: string[] = [];
      onopen?: () => void;
      onmessage?: (event: { data: ArrayBuffer }) => void;
      onclose?: () => void;
      constructor(public readonly url: string) {
        sockets.push(this);
      }
      send(msg: string) {
        this.sent.push(msg);
      }
    }

    const storage = new Map<string, string>();
    const timers: Array<() => void> = [];
    const written: string[] = [];
    const doc = new MockDocument();
    const initialHtml = "<a href='https://initial.example'>Initial</script></a>";
    const initialLink = doc.el("a", { href: "https://initial.example" }, ["Initial"]);
    doc.root.childNodes.push(initialLink);

    const fakeDocument = {
      documentElement: { scrollWidth: 100, scrollHeight: 200 },
      URL: doc.URL,
      open() {},
      write(html: string) {
        written.push(html);
        doc.root.childNodes.splice(
          0,
          doc.root.childNodes.length,
          doc.el("a", { href: "https://updated.example" }, ["Updated"]),
        );
      },
      close() {},
      querySelectorAll: (selector: string) => doc.querySelectorAll(selector),
      getElementById: (id: string) => doc.getElementById(id),
    };

    const script = htmlPreviewClientScript("ws://127.0.0.1:23625", initialHtml);
    expect(script).not.toContain("</script>");

    const run = new Function(
      "WebSocket",
      "document",
      "window",
      "sessionStorage",
      "requestAnimationFrame",
      "setTimeout",
      script,
    );

    run(
      FakeWebSocket,
      fakeDocument,
      { innerWidth: 100, innerHeight: 100, scrollX: 0, scrollY: 0, scrollTo() {} },
      {
        getItem: (key: string) => storage.get(key) ?? null,
        setItem: (key: string, value: string) => storage.set(key, value),
        removeItem: (key: string) => storage.delete(key),
      },
      (cb: () => void) => cb(),
      (cb: () => void) => {
        timers.push(cb);
        return timers.length;
      },
    );

    expect(initialLink.getAttribute("title")).toBe("https://initial.example");
    expect(sockets).toHaveLength(1);
    sockets[0].onopen?.();
    expect(sockets[0].sent).toEqual(["current"]);

    // Identical HTML returned after initial "current" does not rewrite the document.
    const samePayload = new TextEncoder().encode(`html,${initialHtml}`);
    sockets[0].onmessage?.({ data: samePayload.buffer });
    expect(written).toHaveLength(0);

    // Changed HTML rewrites the document and reapplies link titles.
    const updatedHtml = "<a href='https://updated.example'>Updated</a>";
    const updatedPayload = new TextEncoder().encode(`html,${updatedHtml}`);
    sockets[0].onmessage?.({ data: updatedPayload.buffer });
    expect(written).toEqual([updatedHtml]);
    expect(doc.querySelectorAll("a")[0].getAttribute("title")).toBe("https://updated.example");

    // Reconnect sends "current" again, and skips rewriting if unchanged.
    sockets[0].onclose?.();
    expect(timers).toHaveLength(1);
    timers[0]();
    expect(sockets).toHaveLength(2);
    sockets[1].onopen?.();
    expect(sockets[1].sent).toEqual(["current"]);
    sockets[1].onmessage?.({ data: updatedPayload.buffer });
    expect(written).toHaveLength(1);
  });
});
