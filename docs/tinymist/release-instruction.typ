#import "mod.typ": *

#show: book-page.with(title: "Release Instructions")

Normally, you should always create release candidates to avoid failures in the release process. For example, if you are releasing version `0.12.19`, you should create a release candidate `0.12.19-rc1` first. This is because once you tag a version, you cannot delete it, otherwise the people in downstream that already pull the tag will have to force update their repository. The release candidates are canaries to test any potential poison in the release process. Two things to note:
- At most 9 release candidates can be created for a version. This is because semver compares the version number as a string, and `rc9` is greater than `rc10` in sense of string comparison.
- You must publish the release soon after a good release candidate is created, otherwise CI may fail tomorrow.

The steps to release are list as following:
- Checking before releases.
- Making a release PR.
- Tagging and pushing current revision to release

#set heading(numbering: numbly("Step {1}~"))

= Checking before Releases

== Checking the `Cargo.toml` and the `Cargo.lock`

A `git` with `branch` dependency is forbidden in the `Cargo.toml` file. This will cause the `Cargo.lock` file to be unstable and the build to fail. Use the `git` with `tag` dependencies instead.

== Checking publish tokens

Please check the deadline of the publish tokens stored in the GitHub secrets. If the tokens are expired, please renew them before release.

- Renew the `VSCODE_MARKETPLACE_TOKEN` according to the #link("https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows")[Azure DevOps -- Use personal access tokens.]
- Renew the `OPENVSX_ACCESS_TOKEN` at the #link("https://open-vsx.org/user-settings/tokens")[Open VSX Registry -- Access Tokens.]

= Making a Release PR

You should perform following steps to make a release PR  with name in format of `build: bump version to {version}`:

== Determining the Version Number

Before release, you should determine the version number to release.

#include "versioning.typ"

== Updating Version String in Codebase

Update Version String in Codebase other than that of `tinymist-assets`, which will be released in the `tinymist::assets::publish` CI. You can `grep` the version number in the repository to check if all the version numbers in the `Cargo.toml` and `package.json` files are updated. Some CI script will also assert failing to help you catch the issue.

== Updating the Changelog

All released version must be documented in the changelog. The changelog is located at `editors/vscode/CHANGELOG.md`. Please ensure the correct format otherwise CI will fail.

== Publishing the tinymist-assets crate

Run the `tinymist::assets::publish` CI to release the `tinymist-assets` crate. Ensure that the `tinymist-assets` crate is published to the registry. Please see `Cargo.lock` to check the released crate is used correctly.

After publish, you should update `tinymist-assets` version in the `Cargo.toml` file.

= Tagging and Pushing Current Revision to Release

Push a tag to the repository with the version number. For example, if you are releasing version `0.12.19`, you should run the following command:

```bash
$ git tag v0.12.19
$ git push --tag
```

This step will trigger the `ci.yml` CI to build and publish the VS Code extensions to the marketplace.

= APPENDIX: Manually generating the GitHub Release's Body (Content)

The `tinymist::announce` is run in CI automatically. You could manually run it to generate announcement body of the GitHub release. It first includes the changelog read from the `CHANGELOG.md` file, then attaches the download script and available download links.
