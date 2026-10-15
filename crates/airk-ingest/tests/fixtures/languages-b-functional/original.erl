-module(archive).
-export([render/1]).
-include("original.hrl").
-spec render(integer()) -> integer().
render(Value) -> Value + ?OFFSET.
run() -> render(1).
