# Changelog

## v0.2.4

### New Features

- [`549d339`](https://github.com/noctisynth/semifold/commit/549d33903c8731f334305fc1d57f3291f1437f02): Optimize CI template and add shell install scripts.

## v0.2.3

### Bug Fixes

- [`2505304`](https://github.com/noctisynth/semifold/commit/2505304e487bd00bf1c2c87e2c26909b677de202): Ignore errors reported from IO due to invalid asset file which is a dir.

## v0.2.2

### Bug Fixes

- [`89e5ddd`](https://github.com/noctisynth/semifold/commit/89e5dddf3059dcd69579f431d21dbbf5742c56d8): Skip 422 issue reported from GitHub when trying to create release.

### Chores

- [`bc7de21`](https://github.com/noctisynth/semifold/commit/bc7de21ba72b63b2c558e7d1906517c2301cb153): Release cross-platform artifacts to GitHub Release.

## v0.2.1

### New Features

- [`1996e48`](https://github.com/noctisynth/semifold/commit/1996e485d9b61e837c660d8b5683b6d11cc6f863): Default to create GitHub releases for private packages.

## v0.2.0

### Refactors

- [`86c97d9`](https://github.com/noctisynth/semifold/commit/86c97d9a63cff0931588c434908bcf4fe91f7805): Mark `--dry-run` flag as global options.

### Bug Fixes

- [`cf4c08b`](https://github.com/noctisynth/semifold/commit/cf4c08b72e6003062a18bd077db3f92ea98a86cd): Fix panic if running publish cli on local machine.
- [`c35a34e`](https://github.com/noctisynth/semifold/commit/c35a34e5a3854cc949fac3d2ee9a80778cc0fd12): Add `id-token` permission for GitHub Actions workflow files to support Node.js publish.
- [`5bc444a`](https://github.com/noctisynth/semifold/commit/5bc444a4e4ec5b864d63cef23687aad52cc854d7): Fix Nodejs default publish command.

### New Features

- [`e009c7e`](https://github.com/noctisynth/semifold/commit/e009c7ec0d2908cdf6bf11430a7c0db46f8f40ad): Support running commands in dry run mode.
- [`b346aa7`](https://github.com/noctisynth/semifold/commit/b346aa74585fbe4196d303ff5b34934d6b8493b5): Prevent publish process with dirty git working tree.
- [`98e4a7d`](https://github.com/noctisynth/semifold/commit/98e4a7d7ba33a1179fd542fdef0c7a4011ecab64): Sort packages and cache version bumps in version process, fix Rust workspace related packages version bump.
- [`27b53b2`](https://github.com/noctisynth/semifold/commit/27b53b28c15e7056f54e0f61ae8f688cf714e59a): When switching from pre-release mode to production mode, ignore minor and major version bumps and remove only the pre-release tag.
- [`985a9f5`](https://github.com/noctisynth/semifold/commit/985a9f5f7614877d8abf54404112481fa45f4a75): Enhance i18n supports for Semifold CLI

## v0.1.19

### Bug Fixes

- [`1862ba8`](https://github.com/noctisynth/semifold/commit/1862ba8d7df701893a65b9187cdbaf9ecaf20fa0): Fix version bump when version mode changed from pre-release to semantic.

## v0.1.18

### New Features

- [`4856c7d`](https://github.com/noctisynth/semifold/commit/4856c7d14bb2bd3622f9ae29f8b75e5ad2f60165): Improve compatibility to `changesets` and `covector`, allow empty tag key now.
- [`ff9d9a1`](https://github.com/noctisynth/semifold/commit/ff9d9a150e5a968cd4f1d1ab7dcdfb29780e0e35): Block the publish process before pre-checking private packages.

## v0.1.17

### Performance Improvements

- [`940c9fc`](https://github.com/noctisynth/semifold/commit/940c9fcfb0422fd98e239401b01683945011227e): Disable useless features and create release profile for binary size optimizations.

## v0.1.16

### New Features

- [`35dad5f`](https://github.com/noctisynth/semifold/commit/35dad5f2d1b5348b2740cd4269005f52b5ca599b): Support pre-release versioning mode.

### Bug Fixes

- [`9d625f4`](https://github.com/noctisynth/semifold/commit/9d625f4309fe19c60e380a6c64348fe2a83feb48): Fix version parse from changelog startswith `v`.

### Refactors

- [`3f27105`](https://github.com/noctisynth/semifold/commit/3f27105467f33cfcb03b6f62a72f9c912ec8827b): Refactor `semifold` crate to support library mode.

## v0.1.15

### Bug Fixes

- [`c791e93`](https://github.com/noctisynth/semifold/commit/c791e9320694354c34aac2e1f2ad0ec4b596ee1a): Fix remaining issues of project renaming.
- [`3a031ee`](https://github.com/noctisynth/semifold/commit/3a031ee7001923932f1ed6853bfd26e7fd431318): Fix delivered template GitHub Actions workflow files.

### New Features

- [`1e55e71`](https://github.com/noctisynth/semifold/commit/1e55e7132c7e7bc1ef375a15c273405845e404be): Select all packages by default if variant is `patch`.
- [`4774f04`](https://github.com/noctisynth/semifold/commit/4774f04580338ebda64da61b7e6eb24bbdc67d6b): Check if Git repository is dirty or clean before versioning packages.

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
