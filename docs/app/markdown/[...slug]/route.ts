import { documentationOrigin, indexPath, markdownPath, renderMarkdown } from '@/lib/agent-docs';
import { source } from '@/lib/source';

export const dynamic = 'force-static';
export const dynamicParams = false;

export function generateStaticParams() {
  return source.getPages().map((page) => ({ slug: markdownPath(page).slice('/markdown/'.length).split('/') }));
}

export async function GET(_request: Request, { params }: { params: Promise<{ slug: string[] }> }) {
  const { slug } = await params;
  const page = source.getPages().find((candidate) => markdownPath(candidate) === `/markdown/${slug.join('/')}`);
  if (!page) return new Response(null, { status: 404 });
  return new Response(renderMarkdown(page, await page.data.getText('processed')), {
    headers: {
      'Content-Type': 'text/markdown; charset=utf-8',
      Link: `<${documentationOrigin}${indexPath(page.locale ?? 'en')}>; rel="describedby"`,
    },
  });
}
