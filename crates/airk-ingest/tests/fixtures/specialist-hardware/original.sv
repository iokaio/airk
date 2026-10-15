// SPDX-License-Identifier: Apache-2.0
// Original café 🌌 fixture: no elaboration or preprocessing.
`include "receipt_defs.svh"
package receipt_types;
    class Receipt;
        int amount;
    endclass
endpackage
interface receipt_bus(input logic clk);
    logic paid;
endinterface
module ledger #(parameter WIDTH = 8) (input logic clk, output logic paid);
    import receipt_types::*;
    function automatic int record(input int amount);
        return amount + 1;
    endfunction
    task clear();
        paid = 0;
    endtask
    worker child(.clk(clk), .paid(paid));
endmodule
