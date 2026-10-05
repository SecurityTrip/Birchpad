// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// A greeting, stored on chain.
contract Greeting {
    string public name = "world";
    uint256 public times = 2;

    event Changed(address indexed by, string name);

    function rename(string calldata newName) external {
        name = newName;
        emit Changed(msg.sender, newName);
    }
}
