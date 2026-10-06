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

@available(macOS 13, *)
func check(_ text: String) -> Bool {
    #if DEBUG
    print(#function)
    #endif
    let pattern = /[a-z]+/
    return text.contains(pattern)
}

outer: for i in 0..<3 {
    if i == 1 { continue outer }
}
