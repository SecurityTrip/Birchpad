package demo

// A greeting, twice.
case class Greeting(name: String, times: Int = 2):
  def render: String = Seq.fill(times)(s"Hello, $name!").mkString(" ")

object Main:
  def main(args: Array[String]): Unit =
    val greeting = Greeting("world")
    println(greeting.render)

import scala.annotation.tailrec

object Count:
  @tailrec
  def down(n: Int): Int = if n <= 0 then 0 else down(n - 1)
