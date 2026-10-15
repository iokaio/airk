module Archive (render) where
import Data.Char (toUpper)
data Record = Record String
render :: String -> String
render value = map toUpper value
