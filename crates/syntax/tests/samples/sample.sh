#!/bin/bash
# Greets everyone.
for name in "$@"; do
  echo "Hello, ${name}!" > /dev/null
done
exit 0
