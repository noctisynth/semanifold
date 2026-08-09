import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
import Image from 'next/image';
import type { Locale } from '@/lib/i18n';
import { localizedPath } from '@/lib/i18n';

export function baseOptions(
  locale: Locale,
  { docsLink = true }: { docsLink?: boolean } = {},
): BaseLayoutProps {
  return {
    nav: {
      title: (
        <span className="site-wordmark">
          <span className="site-logo" aria-hidden>
            <Image
              className="site-logo-light"
              alt=""
              width={29}
              height={29}
              src="/logo-light.svg"
            />
            <Image
              className="site-logo-dark"
              alt=""
              width={29}
              height={29}
              src="/logo-dark.svg"
            />
          </span>
          <span>Semifold</span>
        </span>
      ),
      url: localizedPath(locale, '/'),
    },
    links: [
      ...(docsLink
        ? [
            {
              text: locale === 'en' ? 'Documentation' : '文档',
              url: localizedPath(locale, '/docs/'),
              active: 'nested-url' as const,
            },
          ]
        : []),
      {
        text: 'GitHub',
        url: 'https://github.com/noctisynth/semifold',
        external: true,
      },
    ],
    githubUrl: 'https://github.com/noctisynth/semifold',
  };
}
