interface TabLabelProps {
  children: React.ReactNode;
  icon: React.FunctionComponent<React.SVGProps<SVGSVGElement>> | string;
}

export function TabLabel({ children, icon }: TabLabelProps) {
  const Icon =
    typeof icon === 'string'
      ? ({ ...props }) => <img src={icon} alt={icon} {...props} />
      : icon;
  return (
    <div className="flex flex-row items-center gap-1">
      <Icon className="size-4" />
      {children}
    </div>
  );
}
