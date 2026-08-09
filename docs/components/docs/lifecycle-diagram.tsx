export function LifecycleDiagram({ items }: { items: string[] }) {
  return (
    <ol className="not-prose my-8 flex min-w-0 list-none flex-col gap-2 p-0 lg:flex-row lg:items-stretch">
      {items.map((item, index) => (
        <li
          className="flex min-w-0 flex-1 flex-col items-center gap-2 lg:flex-row"
          key={item}
        >
          <div className="flex min-h-28 w-full min-w-0 flex-1 flex-col rounded-xl border border-fd-border bg-fd-card p-4 shadow-sm">
            <span className="font-mono text-xs font-semibold text-blue-600">
              {String(index + 1).padStart(2, '0')}
            </span>
            <span className="mt-4 text-sm font-semibold leading-6 text-fd-foreground">
              {item}
            </span>
          </div>
          {index < items.length - 1 ? (
            <span
              className="shrink-0 rotate-90 text-xl text-fd-muted-foreground lg:rotate-0"
              aria-hidden
            >
              →
            </span>
          ) : null}
        </li>
      ))}
    </ol>
  );
}
