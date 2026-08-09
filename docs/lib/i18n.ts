import { defineI18nUI } from 'fumadocs-ui/i18n';

export const i18n = defineI18nUI(
  {
    languages: ['en', 'zh'],
    defaultLanguage: 'en',
    fallbackLanguage: null,
    hideLocale: 'default-locale',
    parser: 'dir',
  },
  {
    en: {
      displayName: 'English',
    },
    zh: {
      displayName: '简体中文',
      'Back to Home(404 page)': '返回首页',
      'Choose a language(language switcher)': '选择语言',
      'Choose a language(language switcher)(aria-label)': '选择语言',
      'Close Search(search dialog)(aria-label)': '关闭搜索',
      'Close Sidebar(aria-label)': '关闭侧边栏',
      'Collapse Sidebar(sidebar)(aria-label)': '收起侧边栏',
      'Copied Text(code block)(aria-label)': '已复制',
      'Copy Anchor Link(heading anchor)(aria-label)': '复制标题链接',
      'Copy Markdown(page actions)': '复制 Markdown',
      'Copy Text(code block)(aria-label)': '复制文本',
      'Dark(theme switcher)(aria-label)': '深色',
      'Edit on GitHub(edit page)': '在 GitHub 上编辑',
      'Hide Sidebar(sidebar)': '隐藏侧边栏',
      'Last updated on(page footer)': '最后更新于',
      'Light(theme switcher)(aria-label)': '浅色',
      'Next Page(pagination)': '下一页',
      'No Headings(table of contents)': '本页没有标题',
      'No results found(search dialog)': '没有找到结果',
      'On this page(table of contents)': '本页内容',
      'Open Search(search trigger)(aria-label)': '打开搜索',
      'Open Sidebar(sidebar)(aria-label)': '打开侧边栏',
      'Page Not Found(404 page)': '页面不存在',
      'Previous Page(pagination)': '上一页',
      'Search(search dialog)': '搜索文档',
      'Search(search trigger)': '搜索',
      'Show Sidebar(sidebar)': '显示侧边栏',
      'System(theme switcher)(aria-label)': '跟随系统',
      'Table of Contents(inline table of contents)': '目录',
      'The page you are looking for might have been removed, had its name changed, or is temporarily unavailable.(404 page)':
        '你访问的页面可能已被移动、重命名或暂时不可用。',
      'Toggle Menu(mobile menu)(aria-label)': '切换菜单',
      'Toggle Theme(theme switcher)(aria-label)': '切换主题',
      'View as Markdown(page actions)': '查看 Markdown',
    },
  },
);

export type Locale = (typeof i18n.languages)[number];

export function localizedPath(locale: Locale, path: string): string {
  if (locale === 'en') return path;
  return path === '/' ? '/zh/' : `/zh${path}`;
}
