module Archive where
import Data.Char (toUpper)
type Label = String
data Record = Record Label
class Display a where
  display :: a -> String
instance Display Record where
  display (Record value) = map toUpper value
