import type { Locale } from '@/lib/i18n';

const labels = {
  en: [
    ['01', 'Code change', 'Make one reviewable change.'],
    ['02', 'Changeset', 'Record packages, bump levels, and intent.'],
    ['03', 'Release plan', 'Preview the immutable plan with smif status.'],
    [
      '04',
      'Version & publish',
      'Apply edits, then publish in dependency order.',
    ],
  ],
  zh: [
    ['01', '代码变更', '完成一个可审查的代码改动。'],
    ['02', 'Changeset', '记录 package、bump level 与发布意图。'],
    ['03', 'Release plan', '用 smif status 预览不可变发布计划。'],
    ['04', '版本与发布', '应用版本修改，再按依赖顺序发布。'],
  ],
} satisfies Record<Locale, string[][]>;

export function ReleaseFlow({ locale }: { locale: Locale }) {
  return (
    <ol className="release-flow">
      {labels[locale].map(([number, title, description]) => (
        <li key={number}>
          <span>{number}</span>
          <div>
            <strong>{title}</strong>
            <p>{description}</p>
          </div>
        </li>
      ))}
    </ol>
  );
}
