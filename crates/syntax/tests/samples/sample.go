package main

import "fmt"

// main prints a greeting.
func main() {
	name := "world"
	fmt.Printf("Hello, %s!\n", name)
}

// Point is a position on a grid.
type Point struct {
	X, Y int
}

func (p Point) Sum() int { return p.X + p.Y }
