-module(greeting).
-export([render/1]).

%% A greeting, twice.
render(Name) when is_list(Name) ->
    Parts = [io_lib:format("Hello, ~s!", [Name]) || _ <- lists:seq(1, 2)],
    lists:flatten(lists:join(" ", Parts)).
