# Changelog

## v0.4.0-alpha.7

### Bug Fixes

- [`fce12df`](https://github.com/noctisynth/semifold/commit/fce12dff6bf2282af11f9129d3620ef852daf1e8): Plan Rust package and workspace dependency version edits as one deterministic batch.

### New Features

- [`38e7011`](https://github.com/noctisynth/semifold/commit/38e7011afb86e2a2cf04c9bb90f1b258bda8df87): Define the side-effect-free ecosystem adapter contract and complete batch edit planning input.
- [`04e0a5e`](https://github.com/noctisynth/semifold/commit/04e0a5e5da84a3e5703ba9eaa839a54b17f178ad): Plan Python manifest and source version edits through the unified release plan without modifying Cargo.toml.

## v0.4.0-alpha.6

### New Features

- [`419d267`](https://github.com/noctisynth/semifold/commit/419d267f7f15a2ff930f5fc9f8cd256735b8b07a): Plan CMake and vcpkg version edits before applying C++ release changes.

## v0.4.0-alpha.5

### Bug Fixes

- [`3b961e0`](https://github.com/noctisynth/semifold/commit/3b961e069ea4bd5b43e5b29bf2f5c5fc39414c9b): Return errors instead of panicking when release planning, configuration, changelog, and resolver invariants are unavailable.

### Refactors

- [`f42b195`](https://github.com/noctisynth/semifold/commit/f42b195058a4f6972f1a7468023c0512f4845d24): Keep recoverable input failures as errors while using documented `expect` calls for verified internal invariants.
- [`56e0686`](https://github.com/noctisynth/semifold/commit/56e06863fb2846497ed0b79417d68f3cf17eb8ca): Enforce the production unwrap policy through shared workspace Clippy configuration while allowing documented internal expects.
- [`09e7af6`](https://github.com/noctisynth/semifold/commit/09e7af6e76bed7a01b72e6a675504ab308732d2f): Plan Rust and Node changelog updates as validated file edits that can safely create a missing changelog.
- [`2014b6e`](https://github.com/noctisynth/semifold/commit/2014b6eb2139643e130b51b4ad7f0223f0e826a0): Remove Rust and Node.js legacy direct version writes in favor of planned file edits.

## v0.4.0-alpha.4

### New Features

- [`9136774`](https://github.com/noctisynth/semifold/commit/91367741414431f6e77bb2b3584e9a84b5c39cd7): Plan Rust manifest version replacements from the complete release VersionMap without writing files.
- [`e6ff3b7`](https://github.com/noctisynth/semifold/commit/e6ff3b7a90f8b6e93832ca7b9da6002c5b1dedd9): Plan Node.js package.json version and internal dependency replacements from the complete release VersionMap without rewriting unrelated JSON.

### Bug Fixes

- [`fc7284b`](https://github.com/noctisynth/semifold/commit/fc7284b604ed488a725f0222865946bea646eb3f): Serialize planned package.json edits with serde_json while preserving object-key order and a trailing newline.
- [`02dc67d`](https://github.com/noctisynth/semifold/commit/02dc67d11f54e1db0cc8a0c32855493218ff8556): Treat package.json files without a version field as version 0.0.0 and insert the planned version when writing.

## v0.4.0-alpha.3

### Bug Fixes

- [`3abdf29`](https://github.com/noctisynth/semifold/commit/3abdf29df5cbbe5542ff72a1f98c276dd74e4406): Add shared deterministic package discovery for init and config synchronization, and fail incomplete workspace scans instead of silently skipping invalid packages.

## v0.4.0-alpha.2

### New Features

- [`5bea426`](https://github.com/noctisynth/semifold/commit/5bea42624aa7ba73034ad0fba8dc1c9c14da7419): Bridge configured Rust, Node.js, Python, and C++ packages into the new cross-ecosystem workspace graph.

## v0.4.0-alpha.1

### New Features

- [`ea7c693`](https://github.com/noctisynth/semifold/commit/ea7c693f4b0c3f9e6c44200bebc13f3febedf159): Discover direct CMake workspace members and order internal target dependencies.

### Bug Fixes

- [`945657b`](https://github.com/noctisynth/semifold/commit/945657baa2b2dcae3bad60f59cba2d1ab1d66568): Only release Rust runtime dependents when their internal dependency version constraints are no longer satisfied.
- [`009d95d`](https://github.com/noctisynth/semifold/commit/009d95d59292f6db4cf7cf8bc2f486594cfeaa44): Preserve package.json and vcpkg.json formatting when updating package versions.

### Chores

- [`1860a03`](https://github.com/noctisynth/semifold/commit/1860a0304c443bfafdeeca349fbab3720c0ecd24): Add ecosystem manifest fixtures and snapshots for resolver regression coverage.

## v0.4.0-alpha.0

### Bug Fixes

- [`334977f`](https://github.com/noctisynth/semifold/commit/334977ff31af0b0a0858a82c1e9c383e5f333069): Automatically include transitive Rust dependents in version releases and rewrite their internal dependency versions before post-version commands run.
- [`8a9a56e`](https://github.com/noctisynth/semifold/commit/8a9a56ea402c1e4364c2ed210b3696a3d0b37f73): Fix formatting lint violations reported by Rust 1.97.

### New Features

- [`1f2498c`](https://github.com/noctisynth/semifold/commit/1f2498cf34d99663199d49a1ab9bbc7d88a34c1c): Add alpha release-channel lifecycle support, including stable-base selection, in-channel sequencing, and channel switching.

## v0.3.5

### Bug Fixes

- [`65a53a7`](https://github.com/noctisynth/semifold/commit/65a53a7a5e121f0fa52e258f14681aa727a473c9): Auto add version field for path based dependencies.

## v0.3.4

### Bug Fixes

- [`1eb7732`](https://github.com/noctisynth/semifold/commit/1eb7732b230b9c809e292f6ec3324e3eb7dfba34): Ensure all assets filtered by glob patterns are files.

## v0.3.3

### Bug Fixes

- [`df6e2ab`](https://github.com/noctisynth/semifold/commit/df6e2abd48beff959570d9cce997a7a00c829ee9): Always resolve asset files and use full path glob pattern instead.

## v0.3.2

### New Features

- [`fd41853`](https://github.com/noctisynth/semifold/commit/fd41853260fbb5b1e61a41373c24684d2a38e22e): Support search upload assets by glob pattern.

## v0.3.1

### Bug Fixes

- [`8a26838`](https://github.com/noctisynth/semifold/commit/8a2683871626a57a4e3b80788c8f151d8fde9a76): Fix rust private flag.
- [`306d737`](https://github.com/noctisynth/semifold/commit/306d7375fb2da7adabf9ad4b268e119674732c17): Fix resolver display names, use camel case instead.

## v0.3.0

### New Features

- [`d8959d0`](https://github.com/noctisynth/semifold/commit/d8959d02b980e2407fa95009e8afbf4c4375b1c0): 1. Add base_url field to RepoInfo struct 2. Read GITHUB_SERVER_URL env var with fallback to <https://github.com> 3. Use dynamic URL in changelog commit links ([#58](https://github.com/noctisynth/semifold/pull/58) by @BegoniaHe)

### Bug Fixes

- [`d8959d0`](https://github.com/noctisynth/semifold/commit/d8959d02b980e2407fa95009e8afbf4c4375b1c0): Convert `parts` from Vec<&str> to Vec<String> in bump_prerelease function ([#58](https://github.com/noctisynth/semifold/pull/58) by @BegoniaHe)
- [`d8959d0`](https://github.com/noctisynth/semifold/commit/d8959d02b980e2407fa95009e8afbf4c4375b1c0): Rust projects may not have a [dependencies] section (e.g., pure library crates or those with only dev-dependencies). This change makes the dependencies table optional instead of requiring it. ([#58](https://github.com/noctisynth/semifold/pull/58) by @BegoniaHe)

### Refactors

- [`d8959d0`](https://github.com/noctisynth/semifold/commit/d8959d02b980e2407fa95009e8afbf4c4375b1c0): 1. Add optional clap dependency to semifold-resolver 2. Conditionally derive ValueEnum on ResolverType 3. Remove duplicate ResolverType definition in init.rs ([#58](https://github.com/noctisynth/semifold/pull/58) by @BegoniaHe)

## v0.2.2

### New Features

- [`d39c94a`](https://github.com/noctisynth/semifold/commit/d39c94a3b30df9640fb147c77a820a87c9167319): bump version for semifold cpp support ([#52](https://github.com/noctisynth/semifold/pull/52) by @BegoniaHe)

## v0.2.1

### New Features

- [`45b6ab3`](https://github.com/noctisynth/semifold/commit/45b6ab314430aa410d44ad2c545518773d812337): Fix the order of JSON fields when bumping versions.

## v0.2.0

### Bug Fixes

- [`cd86453`](https://github.com/noctisynth/semifold/commit/cd86453841b6d394d9281c0412ad5e75794b85a9): Fix git repo status check for `git2::Status::IGNORE` files.
- [`8e8bf97`](https://github.com/noctisynth/semifold/commit/8e8bf97acefef3faf4817957acb12ec0f91dd93a): Fix glob pattern on non-posix platforms.

### Refactors

- [`86c97d9`](https://github.com/noctisynth/semifold/commit/86c97d9a63cff0931588c434908bcf4fe91f7805): Mark `--dry-run` flag as global options.

### New Features

- [`e009c7e`](https://github.com/noctisynth/semifold/commit/e009c7ec0d2908cdf6bf11430a7c0db46f8f40ad): Support running commands in dry run mode.
- [`98e4a7d`](https://github.com/noctisynth/semifold/commit/98e4a7d7ba33a1179fd542fdef0c7a4011ecab64): Sort packages and cache version bumps in version process, fix Rust workspace related packages version bump.
- [`27b53b2`](https://github.com/noctisynth/semifold/commit/27b53b28c15e7056f54e0f61ae8f688cf714e59a): When switching from pre-release mode to production mode, ignore minor and major version bumps and remove only the pre-release tag.

## v0.1.17

### Bug Fixes

- [`1862ba8`](https://github.com/noctisynth/semifold/commit/1862ba8d7df701893a65b9187cdbaf9ecaf20fa0): Fix version bump when version mode changed from pre-release to semantic.
- [`1cd6143`](https://github.com/noctisynth/semifold/commit/1cd6143b4b0c87cb33b42b085da8149262b5ef53): Nodejs resolver should include root package when resolving nodejs workspaces.

## v0.1.16

### New Features

- [`4856c7d`](https://github.com/noctisynth/semifold/commit/4856c7d14bb2bd3622f9ae29f8b75e5ad2f60165): Improve compatibility to `changesets` and `covector`, allow empty tag key now.

## v0.1.15

### New Features

- [`35dad5f`](https://github.com/noctisynth/semifold/commit/35dad5f2d1b5348b2740cd4269005f52b5ca599b): Support pre-release versioning mode.

## v0.1.14

### Bug Fixes

- [`eb80fa8`](https://github.com/noctisynth/semifold/commit/eb80fa8ad0cf07b522f6e0f95a55371893788424): Fix current Git repository status check.

### New Features

- [`4774f04`](https://github.com/noctisynth/semifold/commit/4774f04580338ebda64da61b7e6eb24bbdc67d6b): Check if Git repository is dirty or clean before versioning packages.

## v0.1.13

### Chores

- [`dccb0d2`](https://github.com/noctisynth/semifold/commit/dccb0d2312ea31e340a67ab2f6552a3918ce887a): Add readme and authors fields to `Cargo.toml`.

### New Features

- [`1ab8df9`](https://github.com/noctisynth/semifold/commit/1ab8df941408a707ec2ac0ca3c152257b8df7517): enhance dynamic version extraction for Python projects ([#29](https://github.com/noctisynth/semifold/pull/29) by @HsiangNianian)

## v0.1.12

### Bug Fixes

- [`ca8ad93`](https://github.com/noctisynth/semifold/commit/ca8ad93e48e2c87b5267d1769e5ae6b2f7d156d4): Assets should relative to repository root path instead of package root.

## v0.1.11

### New Features

- [`943a27c`](https://github.com/noctisynth/semifold/commit/943a27c26cfdb048b94f9c2e10ac12c6b3705392): Support upload GitHub release assets.

## v0.1.10

### New Features

- [`bbe6419`](https://github.com/noctisynth/semifold/commit/bbe6419bba673fc0e8a1ab7957d62fd0956b27ed): Skip publish private packages.

## v0.1.9

### New Features

- [`235d5f0`](https://github.com/noctisynth/semifold/commit/235d5f0e94b09094abb87caacd93bda46875121a): Support customize standard outputs for `stdout` and `stderr`.

## v0.1.8

### Bug Fixes

- [`6aa9bdf`](https://github.com/noctisynth/semifold/commit/6aa9bdfed57c03ca00bd39d4327409d8ac5087fc): Post version commands should run after all versioning tasks done.
- [`b95d9a5`](https://github.com/noctisynth/semifold/commit/b95d9a5714bb7bd0d4e66a688b0edeb51a34b812): Post version commands run for every package.

## v0.1.7

### New Features

- [`979e7de`](https://github.com/noctisynth/semifold/commit/979e7def35be9c1dd527822ab129f534eacec6ef): Support trigger post version commands after versioning.

## v0.1.6

### New Features

- [`450054a`](https://github.com/noctisynth/semifold/commit/450054ad8b496e1634553589d15815b0d8c8048a): add Python support to resolver ([#17](https://github.com/noctisynth/semifold/pull/17) by @HsiangNianian)

## v0.1.5

### New Features

- [`ee97bad`](https://github.com/noctisynth/semifold/commit/ee97bad45819d73f59f30d36ce0b50b1b4b61e78): Allow default publish fields in config.
- [`0171573`](https://github.com/noctisynth/semifold/commit/0171573c15463971538c85c801227145e4648e7d): Optimize empty config fields default serialization.

## v0.1.4

### New Features

- [`4bf1183`](https://github.com/noctisynth/semifold/commit/4bf11839b609bd6610423ede224fc89923fde079): Support Nodejs workspace resolve and version bumps.

## v0.1.3

### Bug Fixes

- [`66da4e2`](https://github.com/noctisynth/semifold/commit/66da4e2d6c26f8abe710f6a231b623127f3be090): Fix relative paths in packages sorting.

### New Features

- [`66da4e2`](https://github.com/noctisynth/semifold/commit/66da4e2d6c26f8abe710f6a231b623127f3be090): Support pre-check config before publishing packages.
- [`3a031ee`](https://github.com/noctisynth/semifold/commit/3a031ee7001923932f1ed6853bfd26e7fd431318): Embed semifold GitHub Actions workflow files.

## v0.1.2

### Bug Fixes

- [`5e1b994`](https://github.com/noctisynth/semifold/commit/5e1b994178fa662b630d700559cc888892b44813): Fix path of resolved package is relative path.

## v0.1.1

### Refactors

- [`2eb3d67`](https://github.com/noctisynth/semifold/commit/2eb3d67a373a55104562f2eaee7c6ebd33794510): Rewrite init command to support new configs.

### Bug Fixes

- [`2245ab9`](https://github.com/noctisynth/semifold/commit/2245ab96d869e5220d125f440747e035774a8c02): Fix packages release order.

### New Features

- [`d94df17`](https://github.com/noctisynth/semifold/commit/d94df1729f43bf6f159a00ed701e05e75aad2d02): Support create and apply changeset.
- [`1c06e8c`](https://github.com/noctisynth/semifold/commit/1c06e8cbe2f179fe0eb8a657249ba5573b1dfbaf): Use `toml_edit` to replace `toml`.
- [`9174302`](https://github.com/noctisynth/semifold/commit/9174302d76386cabb8de0948729b1e7267cc8e8f): Support `ci` and `status` command. ([#8](https://github.com/noctisynth/semifold/pull/8) by @fu050409)
- [`4007f78`](https://github.com/noctisynth/semifold/commit/4007f789aabf1aecaccb2066899b148edcd8c24b): Support `version` cli command.
- [`166ea37`](https://github.com/noctisynth/semifold/commit/166ea37e3cec9c690c0d23eec8c09067d8d9d38c): Auto generate changelog content while running version command.
