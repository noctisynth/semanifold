import type { ReactNode } from 'react';
import '@/styles/index.css';
import { LocalizedRootProvider } from '@/components/localized-root-provider';
import { siteMetadata } from '@/lib/metadata';

export const metadata = siteMetadata('en');

export default function EnglishRootLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <html lang="en" suppressHydrationWarning data-scroll-behavior="smooth">
      <body>
        <LocalizedRootProvider locale="en">{children}</LocalizedRootProvider>
      </body>
    </html>
  );
}
