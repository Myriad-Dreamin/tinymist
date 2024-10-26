import assert from "assert";

const startsWith = (str, prefix) => str.startsWith(prefix);
const contains = (str, substr) => str.includes(substr);
const endsWith = (str, suffix) => str.endsWith(suffix);

// prettier-ignore
const releaseCond = (github, matrix) =>
  (startsWith(github.ref, 'refs/tags/') && (!contains(github.ref, 'rc') && (endsWith(github.ref, '0') || endsWith(github.ref, '2') || endsWith(github.ref, '4') || endsWith(github.ref, '6') || endsWith(github.ref, '8'))));

// prettier-ignore
const nightlyCond = (github, matrix) =>
  ((startsWith(github.ref, 'refs/tags/') && !((!contains(github.ref, 'rc') && (endsWith(github.ref, '0') || endsWith(github.ref, '2') || endsWith(github.ref, '4') || endsWith(github.ref, '6') || endsWith(github.ref, '8'))))) || (!startsWith(github.ref, 'refs/tags/') && matrix.regular_build == 'true'));

for (const c of ["false", "true"]) {
  assert(
    releaseCond({ ref: "refs/tags/v0.11.20" }, { regular_build: c }),
    `v0.11.20 (rb: ${c}) is a stable release`
  );
  assert(
    !releaseCond({ ref: "refs/tags/v0.11.21" }, { regular_build: c }),
    `v0.11.21 (rb: ${c}) is a nightly release`
  );
  assert(
    !nightlyCond({ ref: "refs/tags/v0.11.20" }, { regular_build: c }),
    `v0.11.20 (rb: ${c}) is a stable release`
  );
  assert(
    nightlyCond({ ref: "refs/tags/v0.11.21" }, { regular_build: c }),
    `v0.11.21 (rb: ${c}) is a nightly release`
  );
}

for (const tag of ["dev", "dev0", "devrc0", "dev1", "devrc1", "devrc"]) {
  assert(
    !releaseCond({ ref: `refs/head/${tag}` }, { regular_build: "true" }),
    `${tag} is a prerelease`
  );
  assert(
    nightlyCond({ ref: `refs/head/${tag}` }, { regular_build: "true" }),
    `${tag} is a prerelease`
  );
  assert(
    !releaseCond({ ref: `refs/head/${tag}` }, { regular_build: "false" }),
    `${tag} is skipped in prerelease`
  );
  assert(
    !nightlyCond({ ref: `refs/head/${tag}` }, { regular_build: "false" }),
    `${tag} is skipped in prerelease`
  );
}
