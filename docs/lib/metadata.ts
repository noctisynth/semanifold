import type { Metadata } from 'next';
import type { Locale } from '@/lib/i18n';
import { source } from '@/lib/source';

export const siteUrl = new URL('https://semifold.noctisynth.org');

const descriptions: Record<Locale, string> = {
  en: 'Plan, version, and publish releases across Rust, Node.js, Python, and C++ workspaces.',
  zh: '统一规划、版本化并发布 Rust、Node.js、Python 与 C++ workspace。',
};

export function siteMetadata(locale: Locale): Metadata {
  const home = locale === 'en' ? '/' : '/zh/';
  return {
    metadataBase: siteUrl,
    title: {
      default: 'Semifold',
      template: '%s | Semifold',
    },
    description: descriptions[locale],
    icons: {
      icon: [
        { url: '/favicon-light.svg', media: '(prefers-color-scheme: light)' },
        { url: '/favicon-dark.svg', media: '(prefers-color-scheme: dark)' },
      ],
    },
    alternates: {
      canonical: home,
      languages: {
        en: '/',
        'zh-CN': '/zh/',
        'x-default': '/',
      },
    },
    openGraph: {
      type: 'website',
      siteName: 'Semifold',
      title: 'Semifold',
      description: descriptions[locale],
      url: home,
    },
  };
}

export function docsPageMetadata(
  locale: Locale,
  slugs: string[] | undefined,
): Metadata | undefined {
  const page = source.getPage(slugs, locale);
  if (!page) return undefined;

  const en = source.getPage(slugs, 'en')?.url;
  const zh = source.getPage(slugs, 'zh')?.url;

  return {
    title: page.data.title,
    description: page.data.description,
    alternates: {
      canonical: page.url,
      languages: {
        ...(en ? { en, 'x-default': en } : {}),
        ...(zh ? { 'zh-CN': zh } : {}),
      },
    },
    openGraph: {
      type: 'article',
      title: page.data.title,
      description: page.data.description,
      url: page.url,
    },
  };
}
