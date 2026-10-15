# SPDX-License-Identifier: Apache-2.0
defmodule Archive do
  alias String, as: Text
  import Enum, only: [map: 2]
  use GenServer
  require Logger
  def render(value) when is_binary(value) do
    String.upcase(value)
  end
  defmacro stamp(value), do: value
end
