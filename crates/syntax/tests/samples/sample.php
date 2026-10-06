<?php
namespace Demo\Greeting;

const TIMES = 2;
// Greets someone.
function greet(string $name): string {
    return "Hello, $name!";
}
?>
<h1 class="title"><?= greet("world") ?></h1>
