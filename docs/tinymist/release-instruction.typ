#import "mod.typ": *

#show: book-page.with(title: "Release Instructions")

Normally, you should always create release candidates to avoid failures in the release process. For example, if you are releasing version `0.12.19`, you should create a release candidate `0.12.19-rc1` first. This is because once you tag a version, you cannot delete it, otherwise the people in downstream that already pull the tag will have to force update their repository. The release candidates are canaries to test any potential poison in the release process. Two things to note:
- At most 9 release candidates can be created for a version. This is because semver compares the version number as a string, and `rc9` is greater than `rc10` in sense of string comparison.
- You must publish the release soon after a good release candidate is created, otherwise CI may fail tomorrow.

#set heading(numbering: numbly("Step {1}~"))

= Updating Version String to Release

- The `tinymist-assets` package
  - package.json should be the version.
- The VSCode Extension
  - package.json should be the version.
- The Language Server Binaries
  - Cargo.toml should be the version.
- The `tinymist-web` NPM package
  - package.json should be the version.

You can `grep` the version number in the repository to check if all the components are updated. Some CI script will also assert failing to help you catch the issue.

= Updating the Changelog

All released version must be documented in the changelog. The changelog is located at `editors/vscode/CHANGELOG.md`. Please ensure the correct format otherwise CI will fail.

= Generating the GitHub Release's Body (Content)

Run following commands to generate the body of the release announcement:

```bash
$ yarn draft-release 0.12.19
Please check the generated announcement in target/announcement.gen.md
```

The `target/announcement.gen.md` first includes the changelog read from the `CHANGELOG.md` file, then attack the download script and available download links.

= Drafting the Release

Create a draft release on GitHub with the generated announcement.

If you are releasing a nightly version, please set the prerelease flag to true. Otherwise, if you are releasing a regular version, please set the prerelease flag to false. Some package registries relies on this flag to determine whether to update their stable channel.

#include "versioning.typ"

= Checking the `Cargo.toml` and the `Cargo.lock`

A `git` with `branch` dependency is forbidden in the `Cargo.toml` file. This will cause the `Cargo.lock` file to be unstable and the build to fail. Use the `git` with `tag` dependencies instead.

= Checking publish tokens

Please check the deadline of the publish tokens stored in the GitHub secrets. If the tokens are expired, please renew them before release.

- Renew the `VSCODE_MARKETPLACE_TOKEN` according to the #link("https://learn.microsoft.com/en-us/azure/devops/organizations/accounts/use-personal-access-tokens-to-authenticate?view=azure-devops&tabs=Windows")[Azure DevOps -- Use personal access tokens.]
- Renew the `OPENVSX_ACCESS_TOKEN` at the #link("https://open-vsx.org/user-settings/tokens")[Open VSX Registry -- Access Tokens.]

= Publishing the tinymist-assets crate

Ensure that the `tinymist-assets` crate is published to the registry. Please see `Cargo.lock` to check the released crate is used correctly.

= Dry running the CI

Dry running the `release.yml` and the `release-vscode.yml` if you feel necessary.

= Tagging the Release

Push a tag to the repository with the version number. For example, if you are releasing version `0.12.19`, you should run the following command:

```bash
$ git tag v0.12.19
$ git push --tag
```

This step will trigger the `release-vscode.yml` CI to build and publish the VS Code extensions to the marketplace.

= Triggering the Binary Releases

The binary releases is triggered by the `release.yml` CI. You should trigger it after `release-vscode.yml` finished.

The `release.yml` CI will finally undraft the GitHub release automatically that inform everyone the release is ready.
