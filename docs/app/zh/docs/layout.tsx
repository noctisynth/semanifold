import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import type { ReactNode } from 'react';
import { baseOptions } from '@/lib/layout';
import { source } from '@/lib/source';

export default function ChineseDocsLayout({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <DocsLayout
      tree={source.getPageTree('zh')}
      {...baseOptions('zh', { docsLink: false })}
    >
      {children}
    </DocsLayout>
  );
}
