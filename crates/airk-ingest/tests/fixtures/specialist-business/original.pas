unit Archive;
interface
uses SysUtils;
type
  TRecord = class
    function Render(Value: Integer): Integer;
  end;
implementation
function TRecord.Render(Value: Integer): Integer;
begin
  Result := Value + 41;
end;
end.
