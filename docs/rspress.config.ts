import * as path from 'node:path';
import { defineConfig } from 'rspress/config';

export default defineConfig({
  root: path.join(__dirname, 'docs'),
  title:
    'Semifold - Next-generation cross-language monorepo version and release management tool',
  icon: '/noctisynth.png',
  logo: '/noctisynth.png',
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
