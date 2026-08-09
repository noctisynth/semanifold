import { HomeLayout } from 'fumadocs-ui/layouts/home';
import { SiteFooter } from '@/components/docs/site-footer';
import type { Locale } from '@/lib/i18n';
import { baseOptions } from '@/lib/layout';

export function RedirectPage({
  locale,
  destination,
}: {
  locale: Locale;
  destination: string;
}) {
  const copy =
    locale === 'en'
      ? {
          title: 'This documentation page has moved.',
          description: 'Continue to the rewritten Semifold documentation.',
          action: 'Open the new page',
        }
      : {
          title: '该文档页面已移动。',
          description: '请继续前往重写后的 Semifold 文档。',
          action: '打开新页面',
        };

  return (
    <HomeLayout {...baseOptions(locale)}>
      <meta httpEquiv="refresh" content={`0;url=${destination}`} />
      <main className="mx-auto flex min-h-[calc(100vh-3.5rem)] w-[min(100%-2rem,720px)] flex-col items-start justify-center py-20">
        <p className="mb-4 text-xs font-bold uppercase tracking-widest text-blue-600">
          Semifold documentation
        </p>
        <h1 className="m-0 max-w-2xl text-4xl font-semibold tracking-tight text-fd-foreground sm:text-6xl">
          {copy.title}
        </h1>
        <p className="mb-8 mt-5 text-lg text-fd-muted-foreground">
          {copy.description}
        </p>
        <a
          className="inline-flex min-h-12 items-center justify-center rounded-lg bg-blue-600 px-5 font-semibold text-white transition hover:bg-blue-700"
          href={destination}
        >
          {copy.action} →
        </a>
      </main>
      <SiteFooter locale={locale} />
    </HomeLayout>
  );
}
