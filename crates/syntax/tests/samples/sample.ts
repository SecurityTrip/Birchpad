// A greeter.
interface Greeter {
  greet(name: string): string;
}
type Id = number | undefined;
export const greeter: Greeter = { greet: (n) => `Hello, ${n}` };
