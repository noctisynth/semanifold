import * as path from 'node:path';
import { pluginSvgr } from '@rsbuild/plugin-svgr';
import { defineConfig } from 'rspress/config';

export default defineConfig({
  root: path.join(__dirname, 'docs'),
  icon: '/favicon-dark.svg',
  logo: {
    light: '/favicon-light.svg',
    dark: '/favicon-dark.svg',
  },
  logoText: 'Semifold',
  title: 'Semifold',
  description:
    'Next-generation cross-language monorepo versioning and release manager.',

  route: {
    cleanUrls: true,
  },
  lang: 'en',
  themeConfig: {
    locales: [
      {
        lang: 'en',
        label: 'English',
        title: 'Semifold',
        description:
          'Next-generation cross-language monorepo versioning and release manager.',
        searchPlaceholderText: 'Search',
        outlineTitle: 'ON THIS Page',
      },
      {
        lang: 'zh',
        label: '中文',
        title: 'Semifold',
        description: '下一代跨语言单仓库版本管理和发布工具',
        searchPlaceholderText: '搜索',
        outlineTitle: '大纲',
      },
    ],
    socialLinks: [
      {
        icon: 'github',
        mode: 'link',
        content: 'https://github.com/noctisynth/semifold',
      },
    ],
  },
  builderConfig: {
    plugins: [pluginSvgr()],
  },
  globalStyles: path.join(__dirname, 'docs/styles/index.css'),
});
