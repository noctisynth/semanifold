import type { Metadata } from 'next';
import { RedirectPage } from '@/components/docs/redirect-page';

export const metadata: Metadata = {
  robots: { index: false, follow: true },
  alternates: { canonical: '/zh/' },
};

export default function LegacyChineseIndexPage() {
  return <RedirectPage locale="zh" destination="/zh/" />;
}
