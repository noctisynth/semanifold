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
        <span className="inline-flex items-center gap-2.5 font-semibold tracking-tight">
          <span className="relative size-7 shrink-0" aria-hidden>
            <Image
              className="absolute inset-0 size-full dark:hidden"
              alt=""
              width={29}
              height={29}
              src="/favicon-light.svg"
            />
            <Image
              className="absolute inset-0 hidden size-full dark:block"
              alt=""
              width={29}
              height={29}
              src="/favicon-dark.svg"
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
