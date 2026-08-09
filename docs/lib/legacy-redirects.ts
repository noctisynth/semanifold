import type { Locale } from '@/lib/i18n';

export const legacyGuideRedirects = {
  'guide/start': {
    en: '/docs/introduction/',
    zh: '/zh/docs/introduction/',
  },
  'guide/start/quick-start': {
    en: '/docs/getting-started/first-release/',
    zh: '/zh/docs/getting-started/first-release/',
  },
  'guide/commands/init': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/commit': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/status': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/version': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/publish': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/ci': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/commands/mcp': {
    en: '/docs/reference/cli/',
    zh: '/zh/docs/reference/cli/',
  },
  'guide/configuration/config-file': {
    en: '/docs/introduction/',
    zh: '/zh/docs/introduction/',
  },
  'guide/configuration/resolvers': {
    en: '/docs/introduction/',
    zh: '/zh/docs/introduction/',
  },
  'guide/advanced/changeset-format': {
    en: '/docs/introduction/',
    zh: '/zh/docs/introduction/',
  },
} as const;

export type LegacyGuidePath = keyof typeof legacyGuideRedirects;

export const legacyGuidePaths = Object.keys(
  legacyGuideRedirects,
) as LegacyGuidePath[];

export function getLegacyDestination(
  path: string,
  locale: Locale,
): string | undefined {
  const redirect = legacyGuideRedirects[path as LegacyGuidePath];
  return redirect?.[locale];
}
