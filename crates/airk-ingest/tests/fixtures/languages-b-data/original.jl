module Archive
using Dates
import Base: show
struct Record
    title::String
end
function render(value)
    println("café", value)
end
macro original(value)
    value
end
end
