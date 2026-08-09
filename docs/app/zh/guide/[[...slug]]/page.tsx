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
  const destination = getLegacyDestination(`guide/${slug?.join('/')}`, 'zh');
  return {
    robots: { index: false, follow: true },
    alternates: destination ? { canonical: destination } : undefined,
  };
}

export default async function LegacyChineseGuidePage({
  params,
}: {
  params: Promise<{ slug?: string[] }>;
}) {
  const { slug } = await params;
  const destination = getLegacyDestination(`guide/${slug?.join('/')}`, 'zh');
  if (!destination) notFound();
  return <RedirectPage locale="zh" destination={destination} />;
}
