const TOC_SELECTOR = ".package-on-this-page";
const HEADING_SELECTOR = ".package-doc-main h2, .package-doc-main h3, .package-doc-main h4, .package-doc-main h5";

function slug(value, index) {
  return `on-this-page-${index}-${value
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9_-]+/g, "-")
    .replace(/^-+|-+$/g, "") || "section"}`;
}

function headingTarget(heading, index) {
  if (heading.id) {
    return heading.id;
  }

  const child = heading.querySelector("[id]");
  if (child && child.id) {
    return child.id;
  }

  heading.id = slug(heading.textContent || "", index);
  return heading.id;
}

function initToc(toc) {
  if (toc.dataset.ready === "true") {
    return;
  }

  const layout = toc.closest(".package-doc-layout");
  const main = layout && layout.querySelector(".package-doc-main");
  const list = toc.querySelector(".package-on-this-page-list");
  const indicator = toc.querySelector(".package-on-this-page-indicator");
  if (!main || !list || !indicator) {
    return;
  }

  const explicitHeadings = Array.from(main.querySelectorAll("[data-toc-label]"));
  const useExplicitHeadings = explicitHeadings.length > 0;
  const headings = useExplicitHeadings
    ? explicitHeadings
    : Array.from(main.querySelectorAll(HEADING_SELECTOR));

  const items = headings
    .filter((heading) => !heading.closest(".package-source-code"))
    .map((heading, index) => {
      const title = (heading.dataset.tocLabel || heading.textContent || "").trim().replace(/\s+/g, " ");
      if (!title) {
        return null;
      }

      const level = useExplicitHeadings
        ? Number(heading.dataset.tocDepth || 0)
        : Number(heading.tagName.slice(1));

      return {
        heading,
        id: headingTarget(heading, index),
        level,
        title,
      };
    })
    .filter(Boolean);

  if (items.length === 0 || (!useExplicitHeadings && items.length < 2)) {
    toc.hidden = true;
    return;
  }

  list.replaceChildren();

  const links = items.map((item) => {
    const row = document.createElement("li");
    row.className = "package-on-this-page-item";

    const link = document.createElement("a");
    link.className = "package-on-this-page-link";
    link.href = `#${item.id}`;
    link.textContent = item.title;
    link.dataset.active = "false";
    link.dataset.depth = useExplicitHeadings
      ? String(item.level)
      : String(Math.max(0, item.level - items[0].level));
    link.addEventListener("click", () => activate(item.id));

    row.append(link);
    list.append(row);
    return { ...item, link };
  });

  let ticking = false;

  function moveIndicator(link) {
    indicator.style.opacity = "1";
    indicator.style.height = `${Math.max(16, link.offsetHeight - 12)}px`;
    indicator.style.transform = `translateY(${link.offsetTop + 6}px)`;
  }

  function activate(id) {
    let activeLink = null;
    for (const item of links) {
      const active = item.id === id;
      item.link.dataset.active = active ? "true" : "false";
      if (active) {
        activeLink = item.link;
      }
    }

    if (activeLink) {
      moveIndicator(activeLink);
    }
  }

  function currentHeading() {
    if (window.scrollY + window.innerHeight >= document.documentElement.scrollHeight - 2) {
      return links[links.length - 1];
    }

    const offset = Math.min(180, window.innerHeight * 0.28);
    let current = links[0];
    for (const item of links) {
      if (item.heading.getBoundingClientRect().top <= offset) {
        current = item;
      } else {
        break;
      }
    }

    return current;
  }

  function update() {
    ticking = false;
    activate(currentHeading().id);
  }

  function schedule() {
    if (!ticking) {
      ticking = true;
      window.requestAnimationFrame(update);
    }
  }

  toc.dataset.ready = "true";
  toc.hidden = false;
  update();

  window.addEventListener("scroll", schedule, { passive: true });
  window.addEventListener("resize", schedule);
  window.addEventListener("hashchange", schedule);
}

function initAllTocs() {
  document.querySelectorAll(TOC_SELECTOR).forEach(initToc);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", initAllTocs, { once: true });
} else {
  initAllTocs();
}
