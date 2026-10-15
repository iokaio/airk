module Archive = struct
  type record = { title : string }
  let render value = String.uppercase_ascii value
end
open Archive
let caption = "café"
