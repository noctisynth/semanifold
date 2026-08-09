import Link from 'next/link';
import type { Locale } from '@/lib/i18n';
import { localizedPath } from '@/lib/i18n';

const copy = {
  en: {
    copyright: '© 2026 Semifold contributors',
    license: 'Released under AGPL-3.0-only.',
    docs: 'Documentation',
    repository: 'GitHub',
    licenseLink: 'License',
  },
  zh: {
    copyright: '© 2026 Semifold 贡献者',
    license: '本项目以 AGPL-3.0-only 许可发布。',
    docs: '文档',
    repository: 'GitHub',
    licenseLink: '开源许可',
  },
} satisfies Record<Locale, Record<string, string>>;

export function SiteFooter({
  locale,
  compact = false,
}: {
  locale: Locale;
  compact?: boolean;
}) {
  const text = copy[locale];

  return (
    <footer
      className={
        compact
          ? 'mt-12 border-t border-fd-border py-8 text-sm text-fd-muted-foreground'
          : 'border-t border-fd-border bg-fd-background px-6 py-10 text-sm text-fd-muted-foreground'
      }
    >
      <div
        className={
          compact
            ? 'flex flex-col gap-5 sm:flex-row sm:items-end sm:justify-between'
            : 'mx-auto flex max-w-6xl flex-col gap-5 sm:flex-row sm:items-end sm:justify-between'
        }
      >
        <div className="space-y-1">
          <p className="font-medium text-fd-foreground">{text.copyright}</p>
          <p>{text.license}</p>
        </div>
        <nav className="flex flex-wrap gap-x-5 gap-y-2" aria-label="Footer">
          <Link
            className="transition-colors hover:text-fd-foreground"
            href={localizedPath(locale, '/docs/')}
          >
            {text.docs}
          </Link>
          <a
            className="transition-colors hover:text-fd-foreground"
            href="https://github.com/noctisynth/semifold"
          >
            {text.repository}
          </a>
          <a
            className="transition-colors hover:text-fd-foreground"
            href="https://github.com/noctisynth/semifold/blob/main/LICENSE"
          >
            {text.licenseLink}
          </a>
        </nav>
      </div>
    </footer>
  );
}
