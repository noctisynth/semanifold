import type { ReactNode } from 'react';

type AvailabilityStatus = 'released' | 'next' | 'planned';

export function Availability({
  status,
  children,
}: {
  status: AvailabilityStatus;
  children: ReactNode;
}) {
  return (
    <aside className={`availability availability-${status}`}>
      <span className="availability-mark" aria-hidden />
      <div>{children}</div>
    </aside>
  );
}
