package demo

import kotlin.math.max

// A greeting, twice.
data class Greeting(val name: String, val times: Int = 2)

fun Greeting.render(): String =
    (1..max(times, 1)).joinToString(" ") { "Hello, $name!" }

fun main() {
    val greeting = Greeting("world")
    println(greeting.render())
}
