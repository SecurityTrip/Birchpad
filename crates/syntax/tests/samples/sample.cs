using System;
// A greeter.
public class Greeter {
    private readonly string name = "world";
    public void Greet() => Console.WriteLine($"Hello, {name}!");
}

namespace Tools
{
    [Obsolete("Use Greeter")]
    public static class Old
    {
        public const int Count = 3;
        public static string Tab() => "a\tb";
    }
}
