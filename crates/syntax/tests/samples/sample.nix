# A greeting, twice.
{ pkgs ? import <nixpkgs> { } }:

let
  name = "world";
  greet = times: builtins.concatStringsSep " " (builtins.genList (_: "Hello, ${name}!") times);
in
pkgs.writeText "greeting" (greet 2)
