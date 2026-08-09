# Semifold documentation

The documentation site uses Next.js 16, Fumadocs, and Tailwind CSS 4. English pages live under `/docs`; Chinese pages use the same slugs under `/zh/docs`.

## Setup

Install the dependencies:

```sh
pnpm install
```

## Get started

Start the dev server:

```sh
pnpm --filter @semifold/docs dev
```

Build the website for production:

```sh
pnpm --filter @semifold/docs check
```

Preview the production build locally:

```sh
pnpm --filter @semifold/docs preview
```

The production artifact is written to `docs/out`. Content belongs in `content/docs/en` and `content/docs/zh`; both locale trees must keep the same slugs.
