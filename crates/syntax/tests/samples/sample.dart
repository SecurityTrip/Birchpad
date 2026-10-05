import 'dart:math';

/// A greeting, twice.
class Greeting {
  final String name;
  final int times;

  const Greeting(this.name, {this.times = 2});

  String render() => List.filled(max(times, 1), 'Hello, $name!').join(' ');
}

void main() {
  print(const Greeting('world').render());
}
