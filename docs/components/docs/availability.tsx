import type { ReactNode } from 'react';

type AvailabilityStatus = 'released' | 'next' | 'planned';

const markClasses: Record<AvailabilityStatus, string> = {
  released:
    'bg-emerald-600 shadow-[0_0_0_4px_color-mix(in_srgb,#059669_14%,transparent)]',
  next: 'bg-amber-600 shadow-[0_0_0_4px_color-mix(in_srgb,#d97706_14%,transparent)]',
  planned:
    'bg-violet-500 shadow-[0_0_0_4px_color-mix(in_srgb,#8b5cf6_14%,transparent)]',
};

export function Availability({
  status,
  children,
}: {
  status: AvailabilityStatus;
  children: ReactNode;
}) {
  return (
    <aside className="my-6 grid grid-cols-[auto_minmax(0,1fr)] gap-3 rounded-lg border border-fd-border bg-fd-muted p-4 text-[0.93rem] leading-7 text-fd-muted-foreground">
      <span
        className={`mt-2 size-2 rounded-full ${markClasses[status]}`}
        aria-hidden
      />
      <div className="[&>:first-child]:mt-0 [&>:last-child]:mb-0">
        {children}
      </div>
    </aside>
  );
}
