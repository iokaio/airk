namespace Archive
open System
type Record() =
    member this.Render(value: int) = value + 41
module Commands =
    let render value = value + 41
