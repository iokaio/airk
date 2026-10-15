# SPDX-License-Identifier: Apache-2.0
library(stats)
requireNamespace("utils")
caption <- "café 🦀"
render <- function(value) {
  result <- paste(caption, value)
  print(result)
}
render("archive")
