import { llms } from 'fumadocs-core/source';
import { source } from '@/lib/source';

export const dynamic = 'force-static';

const index = llms(source);

export function GET() {
  const content = [
    '# Semifold documentation',
    '',
    '> Release management for Rust, Node.js, Python, and C++ workspaces.',
    '',
    index.index('en'),
    '',
    '# Semifold 中文文档',
    '',
    index.index('zh'),
  ].join('\n');

  return new Response(content, {
    headers: { 'Content-Type': 'text/plain; charset=utf-8' },
  });
}
