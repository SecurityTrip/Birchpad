// Greets the world.
const greet = (name) => `Hello, ${name}!`;
class Greeter extends Base {
  constructor() { super(); this.count = 42; }
}
export default greet;
