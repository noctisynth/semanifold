import {
  DocsBody,
  DocsDescription,
  DocsPage,
  DocsTitle,
} from 'fumadocs-ui/layouts/docs/page';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import { notFound } from 'next/navigation';
import { SiteFooter } from '@/components/docs/site-footer';
import type { Locale } from '@/lib/i18n';
import { getMDXComponents } from '@/lib/mdx-components';
import { source } from '@/lib/source';

export async function RenderDocsPage({
  locale,
  slugs,
}: {
  locale: Locale;
  slugs: string[] | undefined;
}) {
  const page = source.getPage(slugs, locale);
  if (!page) notFound();

  const Content = page.data.body;

  return (
    <DocsPage
      toc={page.data.toc}
      footer={{ children: <SiteFooter locale={locale} compact /> }}
    >
      <DocsTitle>{page.data.title}</DocsTitle>
      <DocsDescription>{page.data.description}</DocsDescription>
      <DocsBody>
        <Content
          components={getMDXComponents({
            a: createRelativeLink(source, page),
          })}
        />
      </DocsBody>
    </DocsPage>
  );
}
