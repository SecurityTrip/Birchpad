defmodule Greeting do
  @moduledoc "A greeting, twice."

  def render(name, times \\ 2) when is_binary(name) do
    1..times
    |> Enum.map(fn _ -> "Hello, #{name}!" end)
    |> Enum.join(" ")
  end
end

IO.puts(Greeting.render("world"))
:ok

defmodule Greeting.Config do
  @default_times 2
  @enforce_keys [:name]
  defstruct name: nil, times: @default_times

  def default, do: %__MODULE__{name: "world"}
end
