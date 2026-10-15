// SPDX-License-Identifier: Apache-2.0
// Original café 🌌 fixture: no execution or security claims.
pragma solidity ^0.8.20;
import "./receipt.sol";
interface IReceipt { function record(uint amount) external; }
library MathLedger { function twice(uint value) internal pure returns (uint) { return value * 2; } }
contract Ledger is IReceipt {
    struct Receipt { uint amount; }
    event Paid(address indexed sender, uint amount);
    error InvalidAmount(uint amount);
    function record(uint amount) external override {
        if (amount == 0) revert InvalidAmount(amount);
        uint doubled = MathLedger.twice(amount);
        emit Paid(msg.sender, doubled);
    }
}
