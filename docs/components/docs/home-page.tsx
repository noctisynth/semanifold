import { HomeLayout } from 'fumadocs-ui/layouts/home';
import Image from 'next/image';
import Link from 'next/link';
import { SiteFooter } from '@/components/docs/site-footer';
import type { Locale } from '@/lib/i18n';
import { localizedPath } from '@/lib/i18n';
import { baseOptions } from '@/lib/layout';

interface HomeCopy {
  eyebrow: string;
  title: string;
  lead: string;
  primary: string;
  secondary: string;
  valueTitle: string;
  valueLead: string;
  values: Array<{ number: string; title: string; description: string }>;
  ecosystemTitle: string;
  ecosystemLead: string;
  pluginTitle: string;
  pluginDescription: string;
  pluginStatus: string;
  pluginLink: string;
  closing: string;
  closingLink: string;
  repositoryLabel: string;
  lifecycle: [string, string, string];
}

const copy: Record<Locale, HomeCopy> = {
  en: {
    eyebrow: 'Versioning and releases across languages and ecosystems',
    title: 'One repository. Many ecosystems. One release workflow.',
    lead: 'Semifold connects package discovery, changesets, version propagation, changelogs, and dependency-ordered publishing across every language in your repository.',
    primary: 'Get started',
    secondary: 'What is Semifold?',
    valueTitle: 'Treat the repository as one product system',
    valueLead:
      'Each ecosystem keeps its own manifest and registry rules. Semifold handles the relationships between them.',
    values: [
      {
        number: '01',
        title: 'Discover the whole repository',
        description:
          'Turn Cargo, npm, Python, C++, and plugin-defined packages into one dependency-aware workspace.',
      },
      {
        number: '02',
        title: 'Version related packages together',
        description:
          'Record a change once, then update affected versions, internal requirements, and changelogs consistently.',
      },
      {
        number: '03',
        title: 'Publish in dependency order',
        description:
          'Use the right command and registry checks for each ecosystem while keeping one recoverable release run.',
      },
    ],
    ecosystemTitle: 'Built in for common ecosystems. Extensible for yours.',
    ecosystemLead:
      'Use the maintained adapters for familiar package formats, or add a repository-local JavaScript plugin without giving it unrestricted access to your machine.',
    pluginTitle: 'Custom ecosystem plugins',
    pluginDescription:
      'Define package discovery, inspection, and version edits with the typed SDK and capability-scoped runtime.',
    pluginStatus: 'Available in 0.3.0-rc.6',
    pluginLink: 'Explore the plugin system',
    closing:
      'Start with the repository you already have. Semifold will help you make its version and release rules explicit.',
    closingLink: 'Make your first release',
    repositoryLabel: 'your repository across multiple languages',
    lifecycle: ['version together', 'write changelogs', 'publish in order'],
  },
  zh: {
    eyebrow: '跨语言单仓库的版本与发布工具',
    title: '一个仓库，多种软件生态，一套版本与发布流程。',
    lead: 'Semifold 把仓库中的软件包发现、变更记录、版本联动、变更日志和按依赖发布连接起来，让不同语言的软件包作为一个整体演进。',
    primary: '开始使用',
    secondary: 'Semifold 是什么？',
    valueTitle: '把整座仓库当作一个产品系统',
    valueLead:
      '每种软件生态保留自己的清单格式和发布规则，Semifold 负责处理它们之间的关系。',
    values: [
      {
        number: '01',
        title: '发现整座仓库的软件包',
        description:
          '把 Cargo、npm、Python、C++ 和插件定义的软件包组织成一张包含依赖关系的工作区图。',
      },
      {
        number: '02',
        title: '让相关软件包一起演进',
        description:
          '只记录一次变更，Semifold 会一致地更新受影响的版本、内部依赖约束和变更日志。',
      },
      {
        number: '03',
        title: '按照依赖关系发布',
        description:
          '为每种生态使用正确的命令和软件包仓库检查，同时保留一条可以恢复的发布流程。',
      },
    ],
    ecosystemTitle: '常见生态直接使用，其他生态通过插件接入。',
    ecosystemLead:
      '常见软件包格式由内置适配器维护；仓库内 JavaScript 插件则能在受限权限下接入自己的清单格式。',
    pluginTitle: '自定义生态插件',
    pluginDescription:
      '使用带类型的 SDK 定义软件包发现、信息读取和版本修改，并由能力受限的运行时执行。',
    pluginStatus: '已随 0.3.0-rc.6 发布',
    pluginLink: '了解插件系统',
    closing:
      '从你已经拥有的仓库开始，让 Semifold 帮你把版本与发布规则变成清楚、可执行的流程。',
    closingLink: '完成第一次发布',
    repositoryLabel: '你的跨语言仓库',
    lifecycle: ['联动版本', '生成变更日志', '按依赖发布'],
  },
};

const repositoryFiles = [
  {
    path: 'crates/core',
    manifest: 'Cargo.toml',
    icon: '/ecosystems/rust.svg',
    name: 'Rust',
  },
  {
    path: 'packages/web',
    manifest: 'package.json',
    icon: '/ecosystems/nodejs.svg',
    name: 'Node.js',
  },
  {
    path: 'bindings/python',
    manifest: 'pyproject.toml',
    icon: '/ecosystems/python.svg',
    name: 'Python',
  },
];

const ecosystems = [
  {
    name: 'Rust',
    detail: 'Cargo.toml · crates.io',
    icon: '/ecosystems/rust.svg',
  },
  {
    name: 'Node.js',
    detail: 'package.json · npm',
    icon: '/ecosystems/nodejs.svg',
  },
  {
    name: 'Python',
    detail: 'pyproject.toml · PyPI',
    icon: '/ecosystems/python.svg',
  },
  {
    name: 'C++',
    detail: 'CMake · vcpkg',
    icon: '/ecosystems/cplusplus.svg',
  },
];

export function LocalizedHomePage({ locale }: { locale: Locale }) {
  const text = copy[locale];
  const firstRelease = localizedPath(
    locale,
    '/docs/getting-started/first-release/',
  );
  const introduction = localizedPath(locale, '/docs/introduction/');
  const plugins = localizedPath(locale, '/docs/plugins/overview/');

  return (
    <HomeLayout {...baseOptions(locale)}>
      <section className="relative overflow-x-clip border-b border-fd-border bg-[radial-gradient(circle_at_80%_10%,color-mix(in_srgb,#8157e8_14%,transparent),transparent_32rem)]">
        <div className="mx-auto grid min-w-0 max-w-7xl items-center gap-12 px-5 py-14 sm:px-6 sm:py-20 lg:grid-cols-[1.08fr_0.92fr] lg:gap-14 lg:py-28">
          <div className="min-w-0">
            <p className="mb-5 text-xs font-bold uppercase tracking-[0.18em] text-blue-600">
              {text.eyebrow}
            </p>
            <h1 className="max-w-4xl break-words text-balance text-4xl font-semibold leading-[1.08] tracking-[-0.045em] text-fd-foreground sm:text-6xl sm:leading-[1.02] sm:tracking-[-0.055em]">
              {text.title}
            </h1>
            <p className="mt-7 max-w-2xl text-pretty text-lg leading-8 text-fd-muted-foreground">
              {text.lead}
            </p>
            <div className="mt-9 flex flex-col gap-3 sm:flex-row">
              <Link
                className="inline-flex min-h-12 items-center justify-center rounded-lg bg-blue-600 px-5 font-semibold text-white shadow-lg shadow-blue-600/20 transition hover:-translate-y-0.5 hover:bg-blue-700"
                href={firstRelease}
              >
                {text.primary}
              </Link>
              <Link
                className="inline-flex min-h-12 items-center justify-center rounded-lg border border-fd-border bg-fd-background/80 px-5 font-semibold text-fd-foreground transition hover:bg-fd-muted"
                href={introduction}
              >
                {text.secondary}
              </Link>
            </div>
          </div>

          <div
            className="min-w-0 overflow-hidden rounded-2xl border border-fd-border bg-fd-card shadow-2xl shadow-violet-500/10"
            aria-label={text.repositoryLabel}
            role="img"
          >
            <div className="flex h-12 items-center gap-2 border-b border-fd-border bg-fd-muted/70 px-4">
              <span className="size-2 rounded-full bg-fd-muted-foreground/30" />
              <span className="size-2 rounded-full bg-fd-muted-foreground/30" />
              <span className="size-2 rounded-full bg-fd-muted-foreground/30" />
              <code className="ml-auto text-xs text-fd-muted-foreground">
                repository/
              </code>
            </div>
            <div className="divide-y divide-fd-border px-5 sm:px-7">
              {repositoryFiles.map((file) => (
                <div
                  className="grid grid-cols-[2rem_minmax(0,1fr)_auto] items-center gap-4 py-5"
                  key={file.path}
                >
                  <Image
                    alt={`${file.name} logo`}
                    height={28}
                    src={file.icon}
                    width={28}
                  />
                  <div className="min-w-0">
                    <p className="truncate font-medium text-fd-foreground">
                      {file.path}
                    </p>
                    <p className="truncate text-sm text-fd-muted-foreground">
                      {file.manifest}
                    </p>
                  </div>
                  <span className="rounded-full border border-fd-border px-2.5 py-1 text-xs text-fd-muted-foreground">
                    {file.name}
                  </span>
                </div>
              ))}
            </div>
            <div className="grid grid-cols-3 divide-x divide-fd-border border-t border-fd-border bg-fd-muted/40 text-center text-xs font-medium text-fd-muted-foreground">
              {text.lifecycle.map((item) => (
                <span className="px-2 py-4" key={item}>
                  {item}
                </span>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-6 py-20 lg:py-24">
        <div className="max-w-3xl">
          <p className="mb-4 text-xs font-bold uppercase tracking-[0.18em] text-blue-600">
            01 / ONE REPOSITORY
          </p>
          <h2 className="text-balance text-3xl font-semibold tracking-tight text-fd-foreground sm:text-5xl">
            {text.valueTitle}
          </h2>
          <p className="mt-5 text-lg leading-8 text-fd-muted-foreground">
            {text.valueLead}
          </p>
        </div>
        <div className="mt-12 grid gap-px overflow-hidden rounded-xl border border-fd-border bg-fd-border md:grid-cols-3">
          {text.values.map((value) => (
            <article className="bg-fd-background p-7" key={value.number}>
              <span className="font-mono text-xs font-semibold text-blue-600">
                {value.number}
              </span>
              <h3 className="mt-8 text-xl font-semibold text-fd-foreground">
                {value.title}
              </h3>
              <p className="mt-3 leading-7 text-fd-muted-foreground">
                {value.description}
              </p>
            </article>
          ))}
        </div>
      </section>

      <section className="border-y border-fd-border bg-fd-muted/50 px-6 py-20 lg:py-24">
        <div className="mx-auto grid max-w-6xl gap-12 lg:grid-cols-[0.9fr_1.1fr] lg:items-center">
          <div>
            <p className="mb-4 text-xs font-bold uppercase tracking-[0.18em] text-blue-600">
              02 / ECOSYSTEMS & PLUGINS
            </p>
            <h2 className="text-balance text-3xl font-semibold tracking-tight text-fd-foreground sm:text-5xl">
              {text.ecosystemTitle}
            </h2>
            <p className="mt-5 text-lg leading-8 text-fd-muted-foreground">
              {text.ecosystemLead}
            </p>
          </div>

          <div className="grid gap-3 sm:grid-cols-2">
            {ecosystems.map((ecosystem) => (
              <div
                className="flex items-center gap-4 rounded-xl border border-fd-border bg-fd-background p-4"
                key={ecosystem.name}
              >
                <Image
                  alt={`${ecosystem.name} logo`}
                  height={34}
                  src={ecosystem.icon}
                  width={34}
                />
                <div>
                  <p className="font-semibold text-fd-foreground">
                    {ecosystem.name}
                  </p>
                  <p className="text-sm text-fd-muted-foreground">
                    {ecosystem.detail}
                  </p>
                </div>
              </div>
            ))}
            <article className="rounded-xl border border-blue-600/30 bg-blue-600/[0.04] p-5 sm:col-span-2">
              <div className="flex flex-wrap items-center gap-3">
                <span className="inline-flex size-10 items-center justify-center rounded-lg bg-blue-600 font-mono text-sm font-bold text-white">
                  {'</>'}
                </span>
                <h3 className="font-semibold text-fd-foreground">
                  {text.pluginTitle}
                </h3>
                <span className="rounded-full bg-emerald-500/15 px-2.5 py-1 text-xs font-semibold text-emerald-700 dark:text-emerald-300">
                  {text.pluginStatus}
                </span>
              </div>
              <p className="mt-4 leading-7 text-fd-muted-foreground">
                {text.pluginDescription}
              </p>
              <Link
                className="mt-4 inline-flex font-semibold text-blue-600 hover:text-blue-700"
                href={plugins}
              >
                {text.pluginLink} →
              </Link>
            </article>
          </div>
        </div>
      </section>

      <section className="mx-auto flex max-w-6xl flex-col gap-6 px-6 py-16 sm:flex-row sm:items-center sm:justify-between">
        <p className="max-w-3xl text-2xl font-medium leading-9 text-fd-foreground">
          {text.closing}
        </p>
        <Link
          className="shrink-0 font-semibold text-blue-600 hover:text-blue-700"
          href={firstRelease}
        >
          {text.closingLink} →
        </Link>
      </section>

      <SiteFooter locale={locale} />
    </HomeLayout>
  );
}
