import van, { type ChildDom } from "vanjs-core";

const { div, h1, p } = van.tags;

interface HeaderProps {
  title?: string;
  description?: string;
  actions?: ChildDom;
}

export const Header = (props: HeaderProps = {}) => () => {
  const {
    title = "Export Tool",
    description = "Configure and export your Typst documents to various formats",
    actions
  } = props;

  return div(
    { class: "card flex flex-col gap-sm", style: "margin-bottom: 1.5rem" },
    div(
      { class: "flex flex-col sm:flex-row sm:justify-between sm:items-center gap-sm" },
      div(
        { class: "flex flex-col gap-xs" },
        h1(
          {
            class: "text-xl font-semibold text-base-content",
            style: "margin: 0; font-size: 1.25rem; font-weight: 600;"
          },
          title
        ),
        p(
          {
            class: "text-desc",
            style: "margin: 0; font-size: 0.875rem;"
          },
          description
        )
      ),
      actions ? div({ class: "flex gap-xs" }, actions) : null
    )
  );
};
