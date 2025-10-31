# Changelog

## v0.1.14

### Chores

- [`dccb0d2`](https://github.com/noctisynth/semifold/commit/dccb0d2312ea31e340a67ab2f6552a3918ce887a): Add readme and authors fields to `Cargo.toml`.

## v0.1.13

### Bug Fixes

- [`ca8ad93`](https://github.com/noctisynth/semifold/commit/ca8ad93e48e2c87b5267d1769e5ae6b2f7d156d4): Assets should relative to repository root path instead of package root.

## v0.1.12

### New Features

- [`943a27c`](https://github.com/noctisynth/semifold/commit/943a27c26cfdb048b94f9c2e10ac12c6b3705392): Support upload GitHub release assets.

## v0.1.11

### New Features

- [`bbe6419`](https://github.com/noctisynth/semifold/commit/bbe6419bba673fc0e8a1ab7957d62fd0956b27ed): Skip publish private packages.

### Performance Improvements

- [`25e643d`](https://github.com/noctisynth/semifold/commit/25e643d3c636c409350ec3214ff148558ee486dc): Use `generate-lockfile --offline` instead of `check` to improve post performance.

## v0.1.10

### New Features

- [`235d5f0`](https://github.com/noctisynth/semifold/commit/235d5f0e94b09094abb87caacd93bda46875121a): Support customize standard outputs for `stdout` and `stderr`.

### Bug Fixes

- [`717539f`](https://github.com/noctisynth/semifold/commit/717539f37698d4a8383e21311730bcfa611885e9): Run post version commands in ci environments.

## v0.1.9

### Bug Fixes

- [`6aa9bdf`](https://github.com/noctisynth/semifold/commit/6aa9bdfed57c03ca00bd39d4327409d8ac5087fc): Post version commands should run after all versioning tasks done.
- [`b95d9a5`](https://github.com/noctisynth/semifold/commit/b95d9a5714bb7bd0d4e66a688b0edeb51a34b812): Post version commands run for every package.

## v0.1.8

### Bug Fixes

- [`08b8a47`](https://github.com/noctisynth/semifold/commit/08b8a470f84fdaa2b32b8392b1b4652478023d4f): Fix auto added args for post version commands.

### New Features

- [`08b8a47`](https://github.com/noctisynth/semifold/commit/08b8a470f84fdaa2b32b8392b1b4652478023d4f): Support local versioning.

## v0.1.7

### New Features

- [`979e7de`](https://github.com/noctisynth/semifold/commit/979e7def35be9c1dd527822ab129f534eacec6ef): Support trigger post version commands after versioning.

### Bug Fixes

- [`353f7ee`](https://github.com/noctisynth/semifold/commit/353f7ee50dc81ca9a6f2e67383a9b5178ed5834f): Fix email of CI git config committer.
- [`979e7de`](https://github.com/noctisynth/semifold/commit/979e7def35be9c1dd527822ab129f534eacec6ef): Fix Git username config, use `github-actions[bot]` instead.

## v0.1.6

### Bug Fixes

- [`baa3816`](https://github.com/noctisynth/semifold/commit/baa3816ad6e4312912d368fda83d848b83db20c3): Fix env issue due to actions triggered by `pull_request_target`. ([#19](https://github.com/noctisynth/semifold/pull/19) by @fu050409)
- [`6a82ae3`](https://github.com/noctisynth/semifold/commit/6a82ae3792e0983f4ecd792aaee169d052f8af54): Fix filter pull requests using GitHub API.

### New Features

- [`9674279`](https://github.com/noctisynth/semifold/commit/96742792d4fc8604651feb212dd3f578c2635c16): Optimize status command output message.
- [`450054a`](https://github.com/noctisynth/semifold/commit/450054ad8b496e1634553589d15815b0d8c8048a): add Python support to resolver ([#17](https://github.com/noctisynth/semifold/pull/17) by @HsiangNianian)

## v0.1.5

### New Features

- [`0171573`](https://github.com/noctisynth/semifold/commit/0171573c15463971538c85c801227145e4648e7d): Optimize empty config fields default serialization.

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
