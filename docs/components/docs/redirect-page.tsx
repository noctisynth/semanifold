import { HomeLayout } from 'fumadocs-ui/layouts/home';
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
      <main className="redirect-page">
        <p>Semifold documentation</p>
        <h1>{copy.title}</h1>
        <p>{copy.description}</p>
        <a className="button button-primary" href={destination}>
          {copy.action} →
        </a>
      </main>
    </HomeLayout>
  );
}
