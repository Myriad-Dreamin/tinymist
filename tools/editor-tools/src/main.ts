// tinymist-app
import "./style.css";
import van, { State } from "vanjs-core";
import { requestInitTemplate, setupVscodeChannel } from "./vscode";
const { div, input, button, a, span } = van.tags;

// const isDarkMode = () =>
//   window.matchMedia?.("(prefers-color-scheme: dark)").matches;

// https://packages.typst.org/preview/thumbnails/charged-ieee-0.1.0-small.webp

const Card = (cls: string, ...content: any) => {
  return div({ class: `tinymist-card ${cls}` }, ...content);
};

interface PackageMeta {
  name: string;
  version: string;
  authors: string[];
  license: string;
  description: string;
  repository: string;
  keywords: string[];
  categories: string[];
  compiler: string;
  template: any;
}

const TemplateList = (
  packages: State<PackageMeta[]>,
  categoryFilter: State<Set<string>>
) => {
  const AuthorItem = (author: string) => {
    // split by <
    const [nameStart, emailRest] = author.split("<");
    const name = nameStart.trim();
    const email = emailRest?.split(">")[0] || "";

    if (!email) {
      return span({ class: `tinymist-author-plain` }, name);
    }

    const href = email.startsWith("@")
      ? `https://github.com/${email.slice(1)}`
      : email.startsWith("https://")
        ? email
        : `mailto:${email}`;

    return a({ class: `tinymist-author`, href }, name);
  };

  const AuthorList = (authors: string[]) => {
    if (authors.length <= 1) {
      return span(
        { class: `tinymist-author-container` },
        ...authors.map(AuthorItem)
      );
    }

    return span(
      { class: `tinymist-author-container` },
      AuthorItem(authors[0]),
      ", ",
      span(
        {
          style: "text-decoration: underline",
          title: authors.slice(1).join(", "),
        },
        "et al."
      )
    );
  };

  const TemplateListItem = (item: PackageMeta) => {
    return Card(
      "template-card",
      div(
        div(
          { style: "float: left" },
          a({ href: item.repository, style: "font-size: 1.2em" }, item.name),
          span(" "),
          span({ style: "font-size: 0.8em" }, "v" + item.version),
          span(" by "),
          AuthorList(item.authors)
        ),
        div(
          {
            style: "float: right",
            class: "tinymist-template-action-container",
          },
          button(
            {
              class: "tinymist-template-action",
              onclick() {
                const packageSpec = `@preview/${item.name}:${item.version}`;
                requestInitTemplate(packageSpec);
              },
            },
            "Create"
          )
        )
      ),
      div({ style: "clear: both" }),
      div({ style: "margin-top: 0.4em" }, item.description)
    );
  };

  return div((_dom?: Element) =>
    div(
      packages.val
        .filter((item) => item.template)
        .filter(runFilterCategory(categoryFilter.val))
        .map(TemplateListItem) || []
    )
  );
};

const SearchBar = () => {
  return input({
    class: "tinymist-search",
    placeholder: "Search templates... (todo: implement search)",
    disabled: true,
  });
};

const CategoryFilter = (categories: State<Set<string>>) => {
  interface Category {
    value: string;
    display?: string;
  }

  const CATEGORIES: Category[] = [
    { value: "all", display: "All" },
    { value: "office", display: "Office" },
    { value: "cv", display: "CV" },
    { value: "presentation", display: "Presentation" },
    { value: "book", display: "Book" },
    { value: "fun", display: "For Fun" },
  ];

  const activating = van.state("all");

  const CategoryButton = (category: Category) => {
    return button(
      {
        class: van.derive(() => {
          const activatingCls =
            category.value === activating.val ? " activated" : "";
          return "tinymist-category-filter-button" + activatingCls;
        }),
        onclick: () => {
          activating.val = category.value;
          //   console.log("activating", category);
          categories.val = new Set([category.value]);
        },
      },
      category.display || category.value
    );
  };

  return div(
    { class: "tinymist-category-filter" },
    ...CATEGORIES.map(CategoryButton)
  );
};

const App = () => {
  const packages: State<any> = van.state([]);
  const categoryFilter: State<Set<string>> = van.state(new Set(["all"]));
  van.derive(async () => {
    const rawPackages = await fetch(
      "https://packages.typst.org/preview/index.json"
    ).then((res) => res.json());

    // collect packages by version
    const packagesIndex = new Map<string, PackageMeta[]>();
    for (const pkg of rawPackages) {
      const name = pkg.name;
      if (!packagesIndex.has(name)) {
        packagesIndex.set(name, []);
      }
      packagesIndex.get(name)!.push(pkg);
    }
    // convert back to array
    const packagesList = Array.from(packagesIndex.entries()).map(([_k, v]) => {
      const versions = v.sort((a, b) => a.version.localeCompare(b.version));
      const lastVersion = versions[versions.length - 1];
      return {
        ...lastVersion,
        versions,
      };
    });

    packages.val = packagesList;
  });

  return div(
    SearchBar(),
    CategoryFilter(categoryFilter),
    TemplateList(packages, categoryFilter)
  );
};

setupVscodeChannel();

van.add(document.querySelector("#tinymist-app")!, App());
function runFilterCategory(categoryFilter: Set<string>) {
  return (value: PackageMeta) => {
    if (categoryFilter.has("all")) {
      return true;
    }
    return value.categories.some((cat) => categoryFilter.has(cat));
  };
}
