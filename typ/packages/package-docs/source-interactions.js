const sourceBlocks = document.querySelectorAll(".package-source-code");

if (sourceBlocks.length > 0) {
  const popup = document.createElement("div");
  popup.className = "package-source-floating-hover";
  popup.hidden = true;
  document.body.append(popup);

  let activeToken = null;
  let hideTimer = 0;
  let popupHovered = false;

  popup.addEventListener("pointerenter", () => {
    popupHovered = true;
    clearHideTimer();
  });

  popup.addEventListener("pointerleave", () => {
    popupHovered = false;
    scheduleHide();
  });

  window.addEventListener(
    "scroll",
    () => {
      if (activeToken && !popup.hidden) {
        positionPopup(activeToken, popup);
      }
    },
    true,
  );
  window.addEventListener("resize", () => {
    if (activeToken && !popup.hidden) {
      positionPopup(activeToken, popup);
    }
    for (const block of sourceBlocks) {
      syncSourceLineNumbers(block);
    }
  });

  for (const block of sourceBlocks) {
    initSourceBlock(block);
  }

  function initSourceBlock(block) {
    const data = block.querySelector("script.package-source-token-data");
    const code = block.querySelector(".package-source-scroll pre code");
    if (!data || !code) {
      return;
    }

    let rawTokens;
    try {
      rawTokens = JSON.parse(data.textContent || "[]");
    } catch {
      return;
    }

    const sourceText = code.textContent || "";
    const lineStarts = computeLineStarts(sourceText);
    const tokens = normalizeTokens(rawTokens, sourceText, lineStarts);
    syncSourceLineNumbers(block, code, sourceText, lineStarts);
    if (tokens.length === 0) {
      return;
    }

    wrapCodeTokens(code, tokens);
    block.classList.add("has-source-interactions");

    block.addEventListener("pointerover", (event) => {
      const token = event.target.closest(".package-source-token.has-hover");
      if (!token || !block.contains(token)) {
        return;
      }

      showPopup(token, tokens, popup);
    });

    block.addEventListener("pointerout", (event) => {
      const token = event.target.closest(".package-source-token.has-hover");
      if (!token || !block.contains(token)) {
        return;
      }
      if (event.relatedTarget && (token.contains(event.relatedTarget) || popup.contains(event.relatedTarget))) {
        return;
      }

      scheduleHide();
    });

    block.addEventListener("focusin", (event) => {
      const token = event.target.closest(".package-source-token.has-hover");
      if (token && block.contains(token)) {
        showPopup(token, tokens, popup);
      }
    });

    block.addEventListener("focusout", (event) => {
      const token = event.target.closest(".package-source-token.has-hover");
      if (!token || !block.contains(token)) {
        return;
      }
      if (event.relatedTarget && (token.contains(event.relatedTarget) || popup.contains(event.relatedTarget))) {
        return;
      }

      scheduleHide();
    });
  }

  function syncSourceLineNumbers(block, code, sourceText, lineStarts) {
    const codeElement = code || block.querySelector(".package-source-scroll pre code");
    if (!codeElement) {
      return;
    }

    const text = sourceText ?? codeElement.textContent ?? "";
    const starts = lineStarts ?? computeLineStarts(text);
    const lineNumbers = block.querySelectorAll(".package-source-line-number");
    if (lineNumbers.length === 0) {
      return;
    }

    const textIndex = buildTextIndex(codeElement);
    for (let line = 0; line < lineNumbers.length; line += 1) {
      const start = starts[line] ?? text.length;
      const end = line + 1 < starts.length ? Math.max(start, starts[line + 1] - 1) : text.length;
      const height = measureTextRangeHeight(codeElement, textIndex, start, end);
      if (height != null) {
        lineNumbers[line].style.height = `${height}px`;
      }
    }
  }

  function computeLineStarts(text) {
    const starts = [0];
    for (let index = 0; index < text.length; index += 1) {
      if (text.charCodeAt(index) === 10) {
        starts.push(index + 1);
      }
    }
    return starts;
  }

  function normalizeTokens(rawTokens, sourceText, lineStarts) {
    const byRange = new Map();
    for (const raw of rawTokens) {
      const range = raw && raw.range;
      if (!range || !range.start || !range.end) {
        continue;
      }

      const start = offsetOf(range.start, sourceText, lineStarts);
      const end = offsetOf(range.end, sourceText, lineStarts);
      if (start == null || end == null || end <= start) {
        continue;
      }

      const hover = typeof raw.hover === "string" && raw.hover.trim() !== "" ? raw.hover : null;
      const href = typeof raw.href === "string" && raw.href.trim() !== "" ? raw.href : null;
      if (!hover && !href) {
        continue;
      }

      const key = `${start}:${end}`;
      const existing = byRange.get(key);
      if (existing) {
        existing.hover ||= hover;
        existing.href ||= href;
      } else {
        byRange.set(key, {
          start,
          end,
          hover,
          href,
        });
      }
    }

    const sorted = [...byRange.values()].sort((left, right) => left.start - right.start || right.end - left.end);
    const filtered = [];
    let previousEnd = 0;
    for (const token of sorted) {
      if (token.start < previousEnd) {
        continue;
      }
      filtered.push(token);
      previousEnd = token.end;
    }

    return filtered;
  }

  function offsetOf(position, sourceText, lineStarts) {
    const line = Number(position.line);
    const character = Number(position.character);
    if (!Number.isInteger(line) || !Number.isInteger(character) || line < 0 || character < 0) {
      return null;
    }
    if (line >= lineStarts.length) {
      return null;
    }

    const lineStart = lineStarts[line];
    const lineEnd = line + 1 < lineStarts.length ? lineStarts[line + 1] - 1 : sourceText.length;
    return Math.min(lineStart + character, lineEnd);
  }

  function wrapCodeTokens(code, tokens) {
    for (let index = 0; index < tokens.length; index += 1) {
      wrapTokenRange(code, tokens[index], index);
    }
  }

  function locateTextOffset(root, offset, preferPrevious) {
    return locateTextIndexOffset(buildTextIndex(root), offset, preferPrevious);
  }

  function buildTextIndex(root) {
    const nodes = [];
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    let cursor = 0;
    while (walker.nextNode()) {
      const node = walker.currentNode;
      const start = cursor;
      cursor += node.nodeValue.length;
      nodes.push({
        node,
        start,
        end: cursor,
      });
    }
    return {
      nodes,
      length: cursor,
    };
  }

  function locateTextIndexOffset(textIndex, offset, preferPrevious) {
    let last = null;
    for (const item of textIndex.nodes) {
      if (offset < item.end || (preferPrevious && offset === item.end)) {
        return {
          node: item.node,
          offset: Math.max(0, Math.min(item.node.nodeValue.length, offset - item.start)),
        };
      }
      if (!preferPrevious && offset === item.start) {
        return { node: item.node, offset: 0 };
      }
      last = item.node;
    }

    if (last && offset === textIndex.length) {
      return {
        node: last,
        offset: last.nodeValue.length,
      };
    }
    return null;
  }

  function measureTextRangeHeight(codeElement, textIndex, startOffset, endOffset) {
    const start = locateTextIndexOffset(textIndex, startOffset, false);
    const end = locateTextIndexOffset(textIndex, endOffset, true);
    const lineHeight = Number.parseFloat(getComputedStyle(codeElement).lineHeight);
    if (!start || !end || !Number.isFinite(lineHeight)) {
      return null;
    }

    if (start.node === end.node && start.offset === end.offset) {
      return lineHeight;
    }

    const range = document.createRange();
    try {
      range.setStart(start.node, start.offset);
      range.setEnd(end.node, end.offset);
    } catch {
      return null;
    }

    const rects = Array.from(range.getClientRects()).filter((rect) => rect.width > 0 || rect.height > 0);
    if (rects.length === 0) {
      return lineHeight;
    }

    const rowTops = [];
    for (const rect of rects) {
      if (!rowTops.some((top) => Math.abs(top - rect.top) < lineHeight / 2)) {
        rowTops.push(rect.top);
      }
    }
    return Math.max(lineHeight, rowTops.length * lineHeight);
  }

  function wrapTokenRange(code, token, index) {
    const start = locateTextOffset(code, token.start, false);
    const end = locateTextOffset(code, token.end, true);
    if (!start || !end) {
      return;
    }

    const range = document.createRange();
    try {
      range.setStart(start.node, start.offset);
      range.setEnd(end.node, end.offset);
    } catch {
      return;
    }

    if (range.collapsed) {
      return;
    }

    const wrapper = createTokenElement(token, index);
    wrapper.append(range.extractContents());
    range.insertNode(wrapper);
  }

  function createTokenElement(token, index) {
    const element = token.href ? document.createElement("a") : document.createElement("span");
    element.className = "package-source-token";
    element.dataset.sourceToken = String(index);
    if (token.hover) {
      element.classList.add("has-hover");
      if (!token.href) {
        element.tabIndex = 0;
      }
    }
    if (token.href) {
      element.classList.add("has-definition");
      element.href = token.href;
    }
    return element;
  }

  function showPopup(tokenElement, tokens, popupElement) {
    const token = tokens[Number(tokenElement.dataset.sourceToken)];
    if (!token || !token.hover) {
      return;
    }

    clearHideTimer();
    activeToken = tokenElement;
    renderHover(popupElement, token.hover);
    popupElement.hidden = false;
    popupElement.style.visibility = "hidden";
    positionPopup(tokenElement, popupElement);
    popupElement.style.visibility = "visible";
  }

  function renderHover(popupElement, markdown) {
    popupElement.textContent = "";
    const container = document.createElement("div");
    container.className = "package-source-hover-content";
    popupElement.append(container);

    const lines = markdown.replace(/\r\n?/g, "\n").split("\n");
    let index = 0;
    while (index < lines.length) {
      const line = lines[index];
      if (line.trim() === "") {
        index += 1;
        continue;
      }

      if (line.startsWith("```")) {
        const language = line.slice(3).trim();
        const codeLines = [];
        index += 1;
        while (index < lines.length && !lines[index].startsWith("```")) {
          codeLines.push(lines[index]);
          index += 1;
        }
        if (index < lines.length) {
          index += 1;
        }
        appendCodeBlock(container, codeLines.join("\n"), language);
        continue;
      }

      if (line === "---") {
        container.append(document.createElement("hr"));
        index += 1;
        continue;
      }

      const heading = line.match(/^(#{1,3})\s+(.+)$/);
      if (heading) {
        const element = document.createElement("div");
        element.className = `package-source-hover-heading level-${heading[1].length}`;
        element.textContent = heading[2];
        container.append(element);
        index += 1;
        continue;
      }

      const paragraph = [line];
      index += 1;
      while (
        index < lines.length &&
        lines[index].trim() !== "" &&
        !lines[index].startsWith("```") &&
        lines[index] !== "---" &&
        !lines[index].match(/^(#{1,3})\s+(.+)$/)
      ) {
        paragraph.push(lines[index]);
        index += 1;
      }
      const element = document.createElement("p");
      element.textContent = paragraph.join("\n");
      container.append(element);
    }
  }

  function appendCodeBlock(container, code, language) {
    const pre = document.createElement("pre");
    const codeElement = document.createElement("code");
    if (language) {
      codeElement.dataset.language = language;
    }
    codeElement.textContent = code;
    pre.append(codeElement);
    container.append(pre);
  }

  function positionPopup(tokenElement, popupElement) {
    const rect = tokenElement.getBoundingClientRect();
    const margin = 8;
    const maxLeft = window.innerWidth - popupElement.offsetWidth - margin;
    let left = Math.max(margin, Math.min(rect.left, maxLeft));
    let top = rect.bottom + margin;
    let placement = "bottom";

    if (top + popupElement.offsetHeight > window.innerHeight - margin) {
      top = rect.top - popupElement.offsetHeight - margin;
      placement = "top";
    }
    if (top < margin) {
      top = margin;
      placement = "bottom";
    }

    popupElement.dataset.placement = placement;
    popupElement.style.left = `${Math.round(left)}px`;
    popupElement.style.top = `${Math.round(top)}px`;
  }

  function scheduleHide() {
    clearHideTimer();
    hideTimer = window.setTimeout(() => {
      if (popupHovered) {
        return;
      }
      popup.hidden = true;
      activeToken = null;
    }, 120);
  }

  function clearHideTimer() {
    if (hideTimer) {
      window.clearTimeout(hideTimer);
      hideTimer = 0;
    }
  }
}
