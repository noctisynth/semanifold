import type { Metadata } from 'next';
import { RenderDocsPage } from '@/components/docs/render-docs-page';
import { docsPageMetadata } from '@/lib/metadata';
import { source } from '@/lib/source';

export const dynamicParams = false;

export function generateStaticParams() {
  return source.getPages('en').map((page) => ({ slug: page.slugs }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata | undefined> {
  const { slug } = await params;
  return docsPageMetadata('en', slug);
}

export default async function DocsPage({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}) {
  const { slug } = await params;
  return <RenderDocsPage locale="en" slugs={slug} />;
}
