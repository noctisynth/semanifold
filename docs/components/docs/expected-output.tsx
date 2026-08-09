import type { ReactNode } from 'react';

export function ExpectedOutput({
  title = 'Expected result',
  children,
}: {
  title?: string;
  children: ReactNode;
}) {
  return (
    <section className="expected-output">
      <p className="expected-output-title">{title}</p>
      <div>{children}</div>
    </section>
  );
}
