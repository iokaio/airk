# Original vendor-language fixtures

These fictional sources are pending P11 qualification; their presence does not enable an adapter or establish vendor/compiler acceptance.

PL/SQL fixtures exercise package spec/body, nested routines, cursors, triggers, SQL names and Unicode. T-SQL explicitly selected `tsql.sql` exercises GO batches, bracketed names, CTE/join aliases, routines and a trigger. ABAP exercises report/include, interface/class declaration and implementation, methods, form and module. Apex exercises a sharing-qualified class, interface reference, method, SOQL candidate and trigger; `ambiguous.cls` is a negative Visual Basic class fixture whose comments and strings must not select Apex.

Pending tests must also derive CRLF inputs, verify exact UTF-8 byte spans, reject fake declarations inside literals/comments, report malformed/truncated input conservatively, enforce fact/node/text budgets, and prove unconfigured `.sql` does not silently acquire either vendor dialect. No fixture requires execution, vendor connectivity or metadata discovery.
