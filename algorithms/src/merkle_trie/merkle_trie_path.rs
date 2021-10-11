// Copyright (C) 2019-2021 Aleo Systems Inc.
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

use crate::{
    errors::MerkleTrieError,
    traits::{MerkleTrieParameters, CRH},
};
use snarkvm_utilities::ToBytes;

use itertools::Itertools;
use std::sync::Arc;

pub type MerkleTrieDigest<P> = <<P as MerkleTrieParameters>::H as CRH>::Output;

#[derive(Clone, Debug)]
pub struct MerkleTriePath<P: MerkleTrieParameters> {
    pub(crate) parameters: Arc<P>,
    /// A Vector of existing sibling children from leaf to root.
    /// (Does NOT including the parents of the leaf being proven)
    pub path: Vec<Vec<MerkleTrieDigest<P>>>,
    /// Location of the parent nodes within each depth of siblings.
    pub traversal: Vec<u8>,
}

impl<P: MerkleTrieParameters> MerkleTriePath<P> {
    pub fn verify<T: ToBytes>(
        &self,
        root_hash: &MerkleTrieDigest<P>,
        key: &[u8],
        value: &T,
    ) -> Result<bool, MerkleTrieError> {
        assert_eq!(self.path.len(), self.traversal.len());

        let mut curr_hash = self.parameters.hash_leaf(&Some(key.to_vec()), &Some(value))?;

        // Check that the given leaf matches the leaf in the membership proof.
        for (_, (index, siblings)) in self.traversal.iter().zip_eq(self.path.iter()).enumerate() {
            let mut node_hashes: Vec<&MerkleTrieDigest<P>> = siblings.iter().map(|x| x).collect();
            node_hashes.insert(*index as usize, &curr_hash);

            curr_hash = self.parameters.hash_node(&node_hashes)?;
        }

        // Check if final hash is root
        if &curr_hash != root_hash {
            return Ok(false);
        }

        Ok(true)
    }
}