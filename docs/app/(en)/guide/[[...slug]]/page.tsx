import type { Metadata } from 'next';
import { notFound } from 'next/navigation';
import { RedirectPage } from '@/components/docs/redirect-page';
import { getLegacyDestination, legacyGuidePaths } from '@/lib/legacy-redirects';

export const dynamicParams = false;

export function generateStaticParams() {
  return legacyGuidePaths.map((path) => ({
    slug: path.replace(/^guide\//, '').split('/'),
  }));
}

export async function generateMetadata({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}): Promise<Metadata> {
  const { slug } = await params;
  const destination = getLegacyDestination(`guide/${slug?.join('/')}`, 'en');
  return {
    robots: { index: false, follow: true },
    alternates: destination ? { canonical: destination } : undefined,
  };
}

export default async function LegacyEnglishGuidePage({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}) {
  const { slug } = await params;
  const destination = getLegacyDestination(`guide/${slug?.join('/')}`, 'en');
  if (!destination) notFound();
  return <RedirectPage locale="en" destination={destination} />;
}
