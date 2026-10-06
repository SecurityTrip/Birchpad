#!/bin/bash
# Greets everyone.
for name in "$@"; do
  echo "Hello, ${name}!" > /dev/null
done
exit 0

readonly TIMES=2
count=$((TIMES + 1))
echo "$HOME has $count"
ls -la /tmp 2>/dev/null
