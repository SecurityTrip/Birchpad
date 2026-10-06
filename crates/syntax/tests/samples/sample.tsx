// A component.
export function Hello({ name }: { name: string }) {
  return <div className="hello">Hello, {name}!</div>;
}

export const Count = () => <span>{1 + 2}</span>;
