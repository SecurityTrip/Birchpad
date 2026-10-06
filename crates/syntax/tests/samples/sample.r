# A greeting, twice.
greet <- function(name, times = 2L) {
  paste(rep(sprintf("Hello, %s!", name), times), collapse = " ")
}

values <- c(1, 2.5, NA)
if (!is.null(values)) {
  print(greet("world"))
}

total <- stats::median(values, na.rm = TRUE)
