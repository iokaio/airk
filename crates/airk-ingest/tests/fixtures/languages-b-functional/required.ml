module type SOURCE = sig val render : string -> string end
exception Missing of string
module Archive = struct
  type label = string
  let render value = String.uppercase_ascii value
end
include Archive
open Archive
