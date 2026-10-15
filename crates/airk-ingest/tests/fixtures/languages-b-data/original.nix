{ lib }:
let
  caption = "café";
  render = name: lib.concatStrings [ caption name ];
  helper = import ./helper.nix;
in { inherit render; }
