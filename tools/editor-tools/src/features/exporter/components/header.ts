import van, { type ChildDom } from "vanjs-core";

const { div, h1, p } = van.tags;

interface HeaderProps {
  title?: string;
  description?: string;
  actions?: ChildDom;
}

export const Header =
  (props: HeaderProps = {}) =>
  () => {
    const {
      title = "Export Tool",
      description = "Configure and export your Typst documents to various formats",
      actions,
    } = props;

    return div(
      { class: "card flex flex-col gap-sm" },
      div(
        { class: "flex flex-col sm:flex-row sm:justify-between sm:items-center gap-sm" },
        div(
          { class: "flex flex-col gap-xs" },
          h1({ class: "text-xl font-semibold text-base-content" }, title),
          p({ class: "text-desc font-sm" }, description),
        ),
        actions ? div({ class: "flex gap-xs" }, actions) : null,
      ),
    );
  };
