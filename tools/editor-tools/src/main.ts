// tinymist-app
import "./style.css";
import van, { ChildDom, State } from "vanjs-core";
import {
  requestSavePackageData,
  requestInitTemplate,
  setupVscodeChannel,
} from "./vscode";
import { AddIcon, HeartIcon } from "./icons";
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
  catState: FilterState
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
    const TemplateAction = (
      icon: ChildDom,
      title: string,
      onclick: () => void
    ) =>
      button(
        {
          class: "tinymist-button tinymist-template-action",
          title,
          onclick,
        },
        icon
      );

    return Card(
      "template-card",
      div(
        a({ href: item.repository, style: "font-size: 1.2em" }, item.name),
        span(" "),
        span({ style: "font-size: 0.8em" }, "v" + item.version),
        span(" by "),
        AuthorList(item.authors)
      ),
      div(
        {
          style:
            "display: flex; align-items: center; gap: 0.25em; margin-top: 0.4em;",
          class: "tinymist-template-actions",
        },
        button(
          {
            class: van.derive(() => {
              const activatingCls = catState.getIsFavorite("preview", item.name)
                ? " activated"
                : "";
              return "tinymist-button tinymist-template-action" + activatingCls;
            }),
            title: van.derive(() =>
              catState.getIsFavorite("preview", item.name)
                ? "Removes from favorite"
                : "Adds to favorite"
            ),
            onclick() {
              catState.negIsFavorite("preview", item.name);
            },
          },
          HeartIcon(16)
        ),
        TemplateAction(AddIcon(16), "Creates project", () => {
          const packageSpec = `@preview/${item.name}:${item.version}`;
          requestInitTemplate(packageSpec);
        }),
        // categories
        item.categories
          .map((cat) => {
            for (const category of CATEGORIES) {
              if (category.value === cat) {
                return category;
              }
            }
            return { value: cat };
          })
          .map(CategoryButton(catState))
      ),
      div({ style: "clear: both" }),
      div({ style: "margin-top: 0.4em" }, item.description)
    );
  };

  function runFilterCategory(categoryFilter: Set<string>) {
    return (value: PackageMeta) => {
      if (categoryFilter.has("all")) {
        return true;
      }
      return value.categories.some((cat) => categoryFilter.has(cat));
    };
  }

  function runFilterFavorite(value: PackageMeta) {
    if (!catState.filterFavorite.val) {
      return true;
    }
    return catState.getIsFavorite("preview", value.name);
  }

  return div((_dom?: Element) =>
    div(
      packages.val
        .filter((item) => item.template)
        .filter(runFilterCategory(catState.categories.val))
        .filter(runFilterFavorite)
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

class FilterState {
  activating = van.state("all");
  categories = van.state(new Set(["all"]));
  filterFavorite: State<boolean>;
  packageUserData: State<any>;

  constructor(packageUserData: any) {
    this.filterFavorite = van.state(Object.keys(packageUserData).length > 0);
    this.packageUserData = van.state(packageUserData);
    console.log("this.packageUserData", this.packageUserData);
  }

  set(category: string) {
    this.activating.val = category;
    //   console.log("activating", category);
    this.categories.val = new Set([category]);
  }

  getIsFavorite(namespace: string, name: string) {
    console.log(this.packageUserData);
    return this.packageUserData.val[namespace]?.[name]?.isFavorite;
  }

  negIsFavorite(namespace: string, name: string) {
    const thisData = {
      ...this.packageUserData.val,
    };
    const ns = (thisData[namespace] ||= {});
    ns[name] = {
      isFavorite: !this.getIsFavorite(namespace, name),
    };
    this.packageUserData.val = thisData;
    requestSavePackageData(thisData);
  }
}

interface Category {
  value: string;
  display?: string;
}

const CATEGORIES: Category[] = [
  { value: "all", display: "All" },
  { value: "office", display: "Office" },
  { value: "cv", display: "CV" },
  { value: "presentation", display: "Presentation" },
  { value: "paper", display: "Paper" },
  { value: "book", display: "Book" },
  { value: "fun", display: "For Fun" },
];

const CategoryButton = (catState: FilterState) => (category: Category) => {
  return button(
    {
      class: van.derive(() => {
        const activatingCls =
          category.value === catState.activating.val ? " activated" : "";
        return "tinymist-button" + activatingCls;
      }),
      title: "Filter by category: " + category.value,
      onclick: () => catState.set(category.value),
    },
    div(
      {
        style: "height: 16px;",
      },
      category.display || category.value
    )
  );
};

const FilterRow = (catState: FilterState) => {
  const favButton = button(
    {
      class: van.derive(() => {
        const activatingCls = catState.filterFavorite.val ? " activated" : "";
        return "tinymist-button" + activatingCls;
      }),
      title: "Filter by favorite state",
      onclick: () =>
        (catState.filterFavorite.val = !catState.filterFavorite.val),
    },
    HeartIcon(16)
  );
  return div(
    { class: "tinymist-category-filter" },
    favButton,
    ...CATEGORIES.map(CategoryButton(catState))
  );
};

const App = () => {
  const packages: State<any> = van.state([]);
  const favoriteState = `{ "touying": { "isFavorite": true } }`;
  const favoritePlaceholders = `:[[preview:FavoritePlaceholder]]:`;
  const catState = new FilterState(
    JSON.parse(
      favoritePlaceholders.startsWith(":")
        ? favoriteState
        : atob(favoritePlaceholders)
    )
  );
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
    FilterRow(catState),
    TemplateList(packages, catState)
  );
};

setupVscodeChannel();

van.add(document.querySelector("#tinymist-app")!, App());
