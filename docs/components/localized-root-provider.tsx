'use client';

import { RootProvider } from 'fumadocs-ui/provider/next';
import { usePathname, useRouter } from 'next/navigation';
import type { ReactNode } from 'react';
import { i18n, type Locale, localePathFromCurrent } from '@/lib/i18n';

export function LocalizedRootProvider({
  locale,
  children,
}: {
  locale: Locale;
  children: ReactNode;
}) {
  const pathname = usePathname();
  const router = useRouter();
  const provider = i18n.provider(locale);

  return (
    <RootProvider
      i18n={{
        ...provider,
        onLocaleChange(target) {
          const path = localePathFromCurrent(target as Locale, pathname);
          router.push(
            `${path}${window.location.search}${window.location.hash}`,
          );
        },
      }}
      search={{ options: { type: 'static', api: '/api/search' } }}
    >
      {children}
    </RootProvider>
  );
}
