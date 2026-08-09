import { HomeLayout } from 'fumadocs-ui/layouts/home';
import Link from 'next/link';
import { ReleaseFlow } from '@/components/docs/release-flow';
import type { Locale } from '@/lib/i18n';
import { localizedPath } from '@/lib/i18n';
import { baseOptions } from '@/lib/layout';

const copy = {
  en: {
    eyebrow: 'Cross-ecosystem release management',
    title: 'One release plan for every package in your workspace.',
    lead: 'Semifold discovers Rust, Node.js, Python, and C++ packages, then turns each changeset into a deterministic version and publish plan.',
    primary: 'Make your first release',
    secondary: 'Understand the workflow',
    version: 'Docs for v0.3.0-rc.5',
    flowTitle: 'A release you can explain before you run it',
    flowLead:
      'The same plan powers status and version. Publish builds a dependency-ordered plan from the versions already written to your manifests.',
    ecosystemTitle: 'Different ecosystems, one release contract',
    ecosystems: [
      [
        'Rust',
        'Cargo workspace discovery, manifest version edits, dependency propagation, and crates.io publishing.',
      ],
      [
        'Node.js',
        'npm workspace discovery, package.json edits, dependency propagation, and dist-tag aware publishing.',
      ],
      [
        'Python',
        'Python project discovery, version edits, dependency propagation, and package index publishing.',
      ],
      [
        'C++',
        'C++ package discovery and version planning alongside the rest of the workspace.',
      ],
    ],
    closing:
      'Start with one complete, non-interactive release path. You can add channels, templates, checks, and automation after the core workflow is clear.',
    closingLink: 'Open the first-release tutorial',
  },
  zh: {
    eyebrow: '跨 ecosystem 发布管理',
    title: '用一个 release plan 管理 workspace 中的所有 package。',
    lead: 'Semifold 发现 Rust、Node.js、Python 与 C++ package，并把每个 changeset 转化为确定性的版本与发布计划。',
    primary: '完成第一次发布',
    secondary: '理解工作流',
    version: '适用于 v0.3.0-rc.5 的文档',
    flowTitle: '执行之前，先把发布计划解释清楚',
    flowLead:
      'status 与 version 使用同一个 plan；publish 则根据 manifest 中已写入的版本，生成按依赖排序的发布计划。',
    ecosystemTitle: '不同 ecosystem，共用一套发布契约',
    ecosystems: [
      [
        'Rust',
        '发现 Cargo workspace、修改 manifest 版本、传播内部依赖并发布到 crates.io。',
      ],
      [
        'Node.js',
        '发现 npm workspace、修改 package.json、传播内部依赖并处理 dist-tag 发布。',
      ],
      [
        'Python',
        '发现 Python 项目、修改版本、传播内部依赖并发布到 package index。',
      ],
      [
        'C++',
        '发现 C++ package，并与 workspace 里的其他 package 一起参与版本规划。',
      ],
    ],
    closing:
      '先走通一条完整、非交互的发布路径。理解核心工作流后，再加入 channel、模板、检查与自动化。',
    closingLink: '打开第一次发布教程',
  },
} satisfies Record<Locale, Record<string, string | string[][]>>;

export function LocalizedHomePage({ locale }: { locale: Locale }) {
  const text = copy[locale];
  const firstRelease = localizedPath(
    locale,
    '/docs/getting-started/first-release/',
  );

  return (
    <HomeLayout {...baseOptions(locale)}>
      <section className="hero-shell">
        <div className="hero-copy">
          <p className="hero-eyebrow">{text.eyebrow as string}</p>
          <h1>{text.title as string}</h1>
          <p className="hero-lead">{text.lead as string}</p>
          <div className="hero-actions">
            <Link className="button button-primary" href={firstRelease}>
              {text.primary as string}
            </Link>
            <Link
              className="button button-secondary"
              href={localizedPath(locale, '/docs/introduction/')}
            >
              {text.secondary as string}
            </Link>
          </div>
          <p className="release-baseline">{text.version as string}</p>
        </div>
        <div className="hero-plan" aria-hidden>
          <div className="plan-window">
            <div className="plan-window-bar">
              <span />
              <span />
              <span />
              <code>smif status</code>
            </div>
            <div className="plan-output">
              <p>RELEASE PLAN</p>
              <div>
                <span>core</span>
                <strong>0.8.0 → 0.9.0</strong>
              </div>
              <div>
                <span>cli</span>
                <strong>1.4.2 → 1.4.3</strong>
              </div>
              <div>
                <span>sdk</span>
                <strong>2.1.0 → 2.2.0</strong>
              </div>
              <p>3 packages · dependency order verified</p>
            </div>
          </div>
        </div>
      </section>

      <section className="home-section">
        <div className="section-heading">
          <p>01 / RELEASE MODEL</p>
          <h2>{text.flowTitle as string}</h2>
          <p>{text.flowLead as string}</p>
        </div>
        <ReleaseFlow locale={locale} />
      </section>

      <section className="home-section home-section-muted">
        <div className="section-heading">
          <p>02 / ECOSYSTEMS</p>
          <h2>{text.ecosystemTitle as string}</h2>
        </div>
        <div className="ecosystem-grid">
          {(text.ecosystems as string[][]).map(([name, description]) => (
            <article key={name}>
              <span>{name.slice(0, 2).toUpperCase()}</span>
              <h3>{name}</h3>
              <p>{description}</p>
            </article>
          ))}
        </div>
      </section>

      <section className="home-cta">
        <p>{text.closing as string}</p>
        <Link href={firstRelease}>{text.closingLink as string} →</Link>
      </section>
    </HomeLayout>
  );
}
