import type { ReactNode } from 'react';

export function ExpectedOutput({
  title = 'Expected result',
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="my-6 rounded-r-lg border border-l-[3px] border-fd-border border-l-blue-600 bg-[color-mix(in_srgb,#356df3_5%,var(--color-fd-muted))] p-4 text-[0.93rem] leading-7 text-fd-muted-foreground">
      <p className="mb-3 mt-0 text-xs font-bold uppercase tracking-wide text-fd-foreground">
        {title}
      </p>
      <div className="[&>:first-child]:mt-0 [&>:last-child]:mb-0">
        {children}
      </div>
    </section>
  );
}
