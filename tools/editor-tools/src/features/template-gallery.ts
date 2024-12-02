// tinymist-app
import van, { ChildDom, State } from "vanjs-core";
import { requestSavePackageData, requestInitTemplate } from "../vscode";
import { AddIcon, HeartIcon } from "../icons";
import { base64Decode } from "../utils";
import MiniSearch from "minisearch";
import type { SearchResult } from "minisearch";
const { div, input, button, a, span } = van.tags;

// const isDarkMode = () =>
//   window.matchMedia?.("(prefers-color-scheme: dark)").matches;

// https://packages.typst.org/preview/thumbnails/charged-ieee-0.1.0-small.webp

const Card = (cls: string, ...content: any) => {
  return div({ class: `tinymist-card ${cls}` }, ...content);
};

interface PackageMeta {
  id: string;
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

  const highlightMatch = (text: string, searchTerm?: string) => {
    if (!searchTerm || !text) return van.tags.span({}, text);
    console.log("searchTerm", searchTerm);

    const regex = new RegExp(`(${searchTerm})`, 'gi');
    const parts = text.split(regex);

    return van.tags.span({}, ...parts.map(part =>
      regex.test(part)
        ? van.tags.span({ class: 'tinymist-highlight' }, part)
        : part
    ));
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
        a({ href: item.repository, style: "font-size: 1.2em" },
          () => {
            const searchTerm = catState.searchSelected.val?.[0]?.queryTerms?.[0];
            return highlightMatch(item.name, searchTerm);
          }
        ),
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
      div({ style: "margin-top: 0.4em" },
        div({}, () => {
          const searchTerm = catState.searchSelected.val?.[0]?.queryTerms?.[0];
          return highlightMatch(item.description, searchTerm);
        })
      )
    );
  };

  function runFilterSearch(searchResult: SearchResult[] | undefined) {
    // console.log("search", searchResult);
    const searchResultMap = new Set(searchResult?.map((result) => result.id));
    return (value: PackageMeta) =>
      searchResult === undefined || searchResultMap.has(value.id);
  }

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
        .filter(runFilterSearch(catState.searchSelected.val))
        .map(TemplateListItem) || []
    )
  );
};

const SearchBar = (packages: State<PackageMeta[]>, catState: FilterState) => {
  const search = van.derive(() => {
    const search = new MiniSearch({
      fields: ["name", "description", "authors", "keywords", "categories"],
    });
    search.addAll(Object.values(packages.val.filter((item) => item.template)));
    return search;
  });

  return input({
    class: "tinymist-search",
    type: "text",
    placeholder: "Search templates...",
    oninput: (e) => {
      const input = e.target as HTMLInputElement;
      if (input.value === "") {
        catState.searchSelected.val = undefined;
        return;
      }
      const results = search.val.search(input.value, { prefix: true });
      catState.searchSelected.val = results;
    },
  });
};

class FilterState {
  activating = van.state("all");
  categories = van.state(new Set(["all"]));
  filterFavorite: State<boolean>;
  packageUserData: State<any>;
  searchSelected = van.state<SearchResult[] | undefined>(undefined);

  constructor(packageUserData: any) {
    this.filterFavorite = van.state(Object.keys(packageUserData).length > 0);
    this.packageUserData = van.state(packageUserData);
  }

  setCategory(category: string) {
    this.activating.val = category;
    this.categories.val = new Set([category]);
  }

  getIsFavorite(namespace: string, name: string) {
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
      onclick: () => catState.setCategory(category.value),
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

export const TemplateGallery = () => {
  const packages: State<any> = van.state([]);
  const favoriteState = `{ "touying": { "isFavorite": true } }`;
  const favoritePlaceholders = `:[[preview:FavoritePlaceholder]]:`;
  const catState = new FilterState(
    JSON.parse(
      favoritePlaceholders.startsWith(":")
        ? favoriteState
        : base64Decode(favoritePlaceholders)
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
        id: `@preview/${lastVersion.name}`,
        versions,
      };
    });

    packages.val = packagesList;
  });

  return div(
    SearchBar(packages, catState),
    FilterRow(catState),
    TemplateList(packages, catState)
  );
};
