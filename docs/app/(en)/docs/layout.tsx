import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import type { ReactNode } from 'react';
import { baseOptions } from '@/lib/layout';
import { source } from '@/lib/source';

export default function EnglishDocsLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <DocsLayout
      tree={source.getPageTree('en')}
      {...baseOptions('en', { docsLink: false })}
    >
      {children}
    </DocsLayout>
  );
}
