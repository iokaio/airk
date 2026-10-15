-- SPDX-License-Identifier: Apache-2.0
-- Original café 🌌 fixture: no elaboration or simulation.
library ieee;
use ieee.std_logic_1164.all;
package receipt_types is
    subtype receipt_bit is std_logic;
end package receipt_types;
entity ledger is
    port (clk : in std_logic; paid : out std_logic);
end entity ledger;
architecture rtl of ledger is
    signal pending : std_logic;
    component worker is
        port (clk : in std_logic; paid : out std_logic);
    end component worker;
begin
    child: worker port map (clk => clk, paid => pending);
    capture: process(clk)
    begin
        if rising_edge(clk) then paid <= pending; end if;
    end process capture;
end architecture rtl;
