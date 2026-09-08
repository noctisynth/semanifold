import { renderIndex } from '@/lib/agent-docs';
import { source } from '@/lib/source';

export const dynamic = 'force-static';

export function GET() {
  return new Response(renderIndex(source.getPages(), 'zh'), {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
}
