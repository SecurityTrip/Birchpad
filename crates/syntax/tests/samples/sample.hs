module Main where

import Data.List (intercalate)

-- | A greeting, twice.
greet :: String -> Int -> String
greet name times = intercalate " " (replicate times ("Hello, " ++ name ++ "!"))

main :: IO ()
main = do
  let name = "world"
  putStrLn (greet name 2)
