import type { Metadata } from 'next';
import { RedirectPage } from '@/components/docs/redirect-page';

export const metadata: Metadata = {
  robots: { index: false, follow: true },
  alternates: { canonical: '/' },
};

export default function LegacyEnglishIndexPage() {
  return <RedirectPage locale="en" destination="/" />;
}
