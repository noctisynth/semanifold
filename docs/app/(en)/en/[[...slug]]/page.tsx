import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { RedirectPage } from '@/components/docs/redirect-page';
import { getLegacyDestination, legacyGuidePaths } from '@/lib/legacy-redirects';

export const dynamicParams = false;

export function generateStaticParams() {
  return [
    { slug: [] },
    { slug: ['index'] },
    ...legacyGuidePaths.map((path) => ({ slug: path.split('/') })),
  ];
}

function destinationFor(slug: string[] | undefined): string | undefined {
  const path = slug?.join('/');
  if (!path || path === 'index') return '/';
  return getLegacyDestination(path, 'en');
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const destination = destinationFor(slug);
  return {
    robots: { index: false, follow: true },
    alternates: destination ? { canonical: destination } : undefined,
  };
}

export default async function LegacyPrefixedEnglishPage({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}) {
  const { slug } = await params;
  const destination = destinationFor(slug);
  if (!destination) notFound();
  return <RedirectPage locale="en" destination={destination} />;
}
