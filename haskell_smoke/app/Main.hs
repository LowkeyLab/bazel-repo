module Main (main) where

import Smoke (answer, greeting)

main :: IO ()
main =
  if answer == 42 && greeting == "rules_haskell smoke test"
    then putStrLn "haskell smoke executable passed"
    else fail "haskell smoke executable failed"
