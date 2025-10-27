# Changelog

## v0.1.4

### New Features

- [`a6229ae`](https://github.com/noctisynth/semifold/commit/a6229ae83fe10204bc5475320b15bc5e9edf66e7): Add help messages for package selection.
- [`cdc749c`](https://github.com/noctisynth/semifold/commit/cdc749cab0e8e1f390f13f521b7be4041b663740): Support Nodejs workspace resolve and version bumps.

## v0.1.3

### New Features

- [`66da4e2`](https://github.com/noctisynth/semifold/commit/66da4e2d6c26f8abe710f6a231b623127f3be090): Support pre-check config before publishing packages.
- [`3a031ee`](https://github.com/noctisynth/semifold/commit/3a031ee7001923932f1ed6853bfd26e7fd431318): Embed semifold GitHub Actions workflow files.

### Bug Fixes

- [`3a031ee`](https://github.com/noctisynth/semifold/commit/3a031ee7001923932f1ed6853bfd26e7fd431318): Fix resolved project path of single crate project.

## v0.1.2

### New Features

- [`5386984`](https://github.com/noctisynth/semifold/commit/538698464bba9f459b38aaa4cb414112716a2e2d): Add GitHub release title.

### Bug Fixes

- [`c4952cf`](https://github.com/noctisynth/semifold/commit/c4952cff31ed999e44210ffe8dddfcd65f9a526a): Ask user for whether continue due to incomplete selection.
- [`2448ba4`](https://github.com/noctisynth/semifold/commit/2448ba4e59db85c912314d5bfab31784e945980d): Skip unchanged packages when generating changelogs.
- [`5e1b994`](https://github.com/noctisynth/semifold/commit/5e1b994178fa662b630d700559cc888892b44813): Fix path of resolved package is relative path.

## v0.1.1

### Bug Fixes

- [`2245ab9`](https://github.com/noctisynth/semifold/commit/2245ab96d869e5220d125f440747e035774a8c02): Fix `ci` command don't create GitHub releases.
- [`2245ab9`](https://github.com/noctisynth/semifold/commit/2245ab96d869e5220d125f440747e035774a8c02): Fix packages release order.

### Refactors

- [`2eb3d67`](https://github.com/noctisynth/semifold/commit/2eb3d67a373a55104562f2eaee7c6ebd33794510): Rewrite init command to support new configs.

### New Features

- [`d94df17`](https://github.com/noctisynth/semifold/commit/d94df1729f43bf6f159a00ed701e05e75aad2d02): Support create and apply changeset.
- [`9174302`](https://github.com/noctisynth/semifold/commit/9174302d76386cabb8de0948729b1e7267cc8e8f): Support `ci` and `status` command. ([#8](https://github.com/noctisynth/semifold/pull/8) by @fu050409)
- [`4007f78`](https://github.com/noctisynth/semifold/commit/4007f789aabf1aecaccb2066899b148edcd8c24b): Support `version` cli command.
- [`7573c58`](https://github.com/noctisynth/semifold/commit/7573c588702f6e8944ecc53999d62a2cdbfa8f67): Support generate changelog.
- [`9cb72e1`](https://github.com/noctisynth/semifold/commit/9cb72e17d8ca486fc0c4090abeddf8c35eb89e6d): Support create GitHub releases while publishing.
- [`475cc70`](https://github.com/noctisynth/semifold/commit/475cc70a2a373a74e844401cda937af194d22ae2): Add i18n translation for commit cli command.
- [`4ba79d7`](https://github.com/noctisynth/semifold/commit/4ba79d70775fb5f46eb3001c8c7dbce494fa5e54): Support Support changeset file name sanitize.
- [`166ea37`](https://github.com/noctisynth/semifold/commit/166ea37e3cec9c690c0d23eec8c09067d8d9d38c): Auto generate changelog content while running version command.
