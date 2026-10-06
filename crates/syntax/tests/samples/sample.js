// Greets the world.
const greet = (name) => `Hello, ${name}!`;
class Greeter extends Base {
  constructor() { super(); this.count = 42; }
}
/** @param {string} name The name. */
const valid = (name) => /^[a-z]+\d*$/i.test(name);
const style = css`color: red;`;
export default greet;

@sealed
class Sealed {}
const tab = "a\tb";

const link = <a href="https://example.org">Example</a>;
