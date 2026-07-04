const HTML_PREVIEW_SCROLL_KEY = "tinymist-html-preview-scroll";

export function handleHtmlPreviewFrame(
  kind: string,
  payload: Uint8Array,
  url: string,
  decoder = new TextDecoder(),
): boolean {
  if (kind === "html") {
    replaceWithHtmlPreviewDocument(decoder.decode(payload), url);
    return true;
  }

  return false;
}

function storeHtmlPreviewScroll() {
  const root = document.documentElement;
  const maxX = Math.max(1, root.scrollWidth - window.innerWidth);
  const maxY = Math.max(1, root.scrollHeight - window.innerHeight);
  sessionStorage.setItem(
    HTML_PREVIEW_SCROLL_KEY,
    JSON.stringify({
      x: window.scrollX,
      y: window.scrollY,
      rx: window.scrollX / maxX,
      ry: window.scrollY / maxY,
    }),
  );
}

function htmlPreviewClientScript(url: string): string {
  return `
(() => {
  const wsUrl = ${JSON.stringify(url)};
  const scrollKey = ${JSON.stringify(HTML_PREVIEW_SCROLL_KEY)};
  const decoder = new TextDecoder();
  const comma = ",".charCodeAt(0);

  function storeScroll() {
    const root = document.documentElement;
    const maxX = Math.max(1, root.scrollWidth - window.innerWidth);
    const maxY = Math.max(1, root.scrollHeight - window.innerHeight);
    sessionStorage.setItem(scrollKey, JSON.stringify({
      x: window.scrollX,
      y: window.scrollY,
      rx: window.scrollX / maxX,
      ry: window.scrollY / maxY,
    }));
  }

  function restoreScroll() {
    const raw = sessionStorage.getItem(scrollKey);
    if (!raw) {
      return;
    }
    sessionStorage.removeItem(scrollKey);
    try {
      const pos = JSON.parse(raw);
      const root = document.documentElement;
      const maxX = Math.max(0, root.scrollWidth - window.innerWidth);
      const maxY = Math.max(0, root.scrollHeight - window.innerHeight);
      window.scrollTo(
        Number.isFinite(pos.rx) ? pos.rx * maxX : pos.x || 0,
        Number.isFinite(pos.ry) ? pos.ry * maxY : pos.y || 0,
      );
    } catch (err) {
      console.warn("failed to restore Typst HTML preview scroll", err);
    }
  }

  function serializeHtmlDocument(doc) {
    const doctype = doc.doctype ? "<!doctype " + doc.doctype.name + ">" : "<!doctype html>";
    return doctype + "\\n" + doc.documentElement.outerHTML;
  }

  function injectClient(html) {
    const current = document.currentScript;
    const source = current && current.textContent ? current.textContent : "";
    if (!source) {
      return html;
    }
    const doc = new DOMParser().parseFromString(html, "text/html");
    const script = doc.createElement("script");
    script.dataset.tinymistHtmlPreviewClient = "";
    script.text = source;
    (doc.body || doc.documentElement).appendChild(script);
    return serializeHtmlDocument(doc);
  }

  function replaceDocument(html) {
    storeScroll();
    document.open();
    document.write(injectClient(html));
    document.close();
  }

  function splitFrame(data) {
    const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array();
    const idx = bytes.indexOf(comma);
    if (idx < 0) {
      return [decoder.decode(bytes), new Uint8Array()];
    }
    return [decoder.decode(bytes.slice(0, idx)).trim(), bytes.slice(idx + 1)];
  }

  function connect() {
    const ws = new WebSocket(wsUrl);
    ws.binaryType = "arraybuffer";
    ws.onmessage = (event) => {
      if (!(event.data instanceof ArrayBuffer)) {
        return;
      }
      const [kind, payload] = splitFrame(event.data);
      const text = decoder.decode(payload);
      if (kind === "html") {
        replaceDocument(text);
      }
    };
    ws.onclose = () => {
      setTimeout(connect, 1000);
    };
  }

  requestAnimationFrame(() => requestAnimationFrame(restoreScroll));
  connect();
})();
`;
}

function serializeHtmlDocument(doc: Document): string {
  const doctype = doc.doctype ? `<!doctype ${doc.doctype.name}>` : "<!doctype html>";
  return `${doctype}\n${doc.documentElement.outerHTML}`;
}

function injectHtmlPreviewClient(html: string, url: string): string {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const script = doc.createElement("script");
  script.dataset.tinymistHtmlPreviewClient = "";
  script.text = htmlPreviewClientScript(url);
  (doc.body || doc.documentElement).appendChild(script);
  return serializeHtmlDocument(doc);
}

function replaceWithHtmlPreviewDocument(html: string, url: string) {
  storeHtmlPreviewScroll();
  document.open();
  document.write(injectHtmlPreviewClient(html, url));
  document.close();
}
