// Copyright (C) 2019-2022 Aleo Systems Inc.
// This file is part of the snarkVM library.

// The snarkVM library is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// The snarkVM library is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with the snarkVM library. If not, see <https://www.gnu.org/licenses/>.

use crate::{instructions::Instruction, Memory, Operation, UnaryOperation};
use snarkvm_circuits::{Parser, ParserResult};
use snarkvm_utilities::{FromBytes, ToBytes};

use core::fmt;
use nom::combinator::map;
use std::io::{Read, Result as IoResult, Write};

// TODO: Remove store as a public facing operation.
/// Stores `operand` into `register`, if `destination` is not already set.
pub struct Store<M: Memory> {
    operation: UnaryOperation<M::Environment>,
}

impl<M: Memory> Operation for Store<M> {
    type Memory = M;

    /// Returns the opcode as a string.
    #[inline]
    fn opcode() -> &'static str {
        "store"
    }

    /// Parses a string into an 'store' operation.
    #[inline]
    fn parse(string: &str, memory: Self::Memory) -> ParserResult<Self> {
        // Parse the operation from the string.
        let (string, operation) = map(UnaryOperation::parse, |operation| Self { operation })(string)?;
        // Initialize the destination register.
        memory.initialize(operation.operation.destination());
        // Return the operation.
        Ok((string, operation))
    }

    /// Evaluates the operation in-place.
    #[inline]
    fn evaluate(&self, memory: &Self::Memory) {
        // Load the value for the operand, and store it into the destination register.
        memory.store(self.operation.destination(), self.operation.operand().load(memory))
    }
}

impl<M: Memory> fmt::Display for Store<M> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.operation)
    }
}

impl<M: Memory> FromBytes for Store<M> {
    fn read_le<R: Read>(mut reader: R) -> IoResult<Self> {
        Ok(Self { operation: UnaryOperation::read_le(&mut reader)? })
    }
}

impl<M: Memory> ToBytes for Store<M> {
    fn write_le<W: Write>(&self, mut writer: W) -> IoResult<()> {
        self.operation.write_le(&mut writer)
    }
}

#[allow(clippy::from_over_into)]
impl<M: Memory> Into<Instruction<M>> for Store<M> {
    /// Converts the operation into an instruction.
    fn into(self) -> Instruction<M> {
        Instruction::Store(self)
    }
}