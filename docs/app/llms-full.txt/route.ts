import { source } from '@/lib/source';
import { renderMarkdown } from '@/lib/agent-docs';

export const dynamic = 'force-static';

export async function GET() {
  const sections = await Promise.all(
    source.getPages().map(async (page) => {
      const markdown = await page.data.getText('processed');
      return renderMarkdown(page, markdown);
    }),
  );

  return new Response(
    ['# Semifold — complete documentation', '', ...sections].join(
      '\n\n---\n\n',
    ),
    { headers: { 'Content-Type': 'text/plain; charset=utf-8' } },
  );
}
