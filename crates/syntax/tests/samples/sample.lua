#!/usr/bin/env lua
-- Greets everyone.
local function greet(name)
  return "Hello, " .. name
end
for i = 1, 3 do print(greet(i)) end

local MAX <const> = 10
local point = { x = 1, y = 2 }
for i = 1, MAX do
  if i % 2 == 0 then goto continue end
  print(i, point.x)
  ::continue::
end
