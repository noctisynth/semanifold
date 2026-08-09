import { RootProvider } from 'fumadocs-ui/provider/next';
import type { ReactNode } from 'react';
import '@/styles/index.css';
import { i18n } from '@/lib/i18n';
import { siteMetadata } from '@/lib/metadata';

export const metadata = siteMetadata('en');

export default function EnglishRootLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning>
      <body>
        <RootProvider
          i18n={i18n.provider('en')}
          search={{ options: { type: 'static', api: '/api/search' } }}
        >
          {children}
        </RootProvider>
      </body>
    </html>
  );
}
