using System;
// A greeter.
public class Greeter {
    private readonly string name = "world";
    public void Greet() => Console.WriteLine($"Hello, {name}!");
}
