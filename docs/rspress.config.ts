import * as path from 'node:path';
import { defineConfig } from 'rspress/config';

export default defineConfig({
  root: path.join(__dirname, 'docs'),
  title:
    'Semifold - Next-generation cross-language monorepo version and release management tool',
  icon: '/favicon-dark.svg',
  logo: {
    light: '/favicon-light.svg',
    dark: '/favicon-dark.svg',
  },
  route: {
    cleanUrls: true,
  },
  themeConfig: {
    socialLinks: [
      {
        icon: 'github',
        mode: 'link',
        content: 'https://github.com/noctisynth/semifold',
      },
    ],
  },
});
