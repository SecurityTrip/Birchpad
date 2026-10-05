program Greeting;

{ A greeting, twice. }
uses SysUtils;

function Render(const Name: string; Times: Integer): string;
var
  I: Integer;
begin
  Result := '';
  for I := 1 to Times do
    Result := Result + Format('Hello, %s! ', [Name]);
end;

begin
  WriteLn(Render('world', 2));
end.
