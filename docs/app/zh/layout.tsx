import type { ReactNode } from 'react';
import '@/styles/index.css';
import { LocalizedRootProvider } from '@/components/localized-root-provider';
import { siteMetadata } from '@/lib/metadata';

export const metadata = siteMetadata('zh');

export default function ChineseRootLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <html lang="zh-CN" suppressHydrationWarning data-scroll-behavior="smooth">
      <body>
        <LocalizedRootProvider locale="zh">{children}</LocalizedRootProvider>
      </body>
    </html>
  );
}
