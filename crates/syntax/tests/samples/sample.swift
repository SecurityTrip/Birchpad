import Foundation

/// A greeting, twice.
struct Greeting {
    let name: String
    var times = 2

    func render() -> String {
        (0..<times).map { _ in "Hello, \(name)!" }.joined(separator: " ")
    }
}

let greeting = Greeting(name: "world")
print(greeting.render())
