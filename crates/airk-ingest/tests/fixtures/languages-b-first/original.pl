#!/usr/bin/env perl
# SPDX-License-Identifier: Apache-2.0
package Archive;
use strict;
use warnings;
require Archive::Support;
sub render {
  my ($value) = @_;
  print "café 🦀", $value;
}
render("archive");
