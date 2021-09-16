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

use snarkvm_algorithms::{crh::PedersenCompressedCRH, define_masked_merkle_tree_parameters};
use snarkvm_curves::{bls12_377::Fr, edwards_bls12::EdwardsProjective as EdwardsBls};
use snarkvm_utilities::ToBytes;

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{
        Display,
        Formatter,
        {self},
    },
    sync::Arc,
};

pub type MerkleTreeCRH = PedersenCompressedCRH<EdwardsBls, 4, 128>;

// We instantiate the tree here with depth = 2. This may change in the future.
pub const MASKED_TREE_DEPTH: usize = 2;

define_masked_merkle_tree_parameters!(MaskedMerkleTreeParameters, MerkleTreeCRH, MASKED_TREE_DEPTH);

/// A Merkle Tree instantiated with the Masked Pedersen hasher over BLS12-377
pub type EdwardsMaskedMerkleTree = MerkleTree<MaskedMerkleTreeParameters>;

/// Lazily evaluated parameters for the Masked Merkle tree
pub static PARAMS: Lazy<Arc<MaskedMerkleTreeParameters>> =
    Lazy::new(|| Arc::new(MaskedMerkleTreeParameters::setup("MerkleTreeParameters")));

/// A Pedersen Merkle Root
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PedersenMerkleRoot(pub [u8; 32]);

impl PedersenMerkleRoot {
    /// Returns the Merkle root for the given leaves using a Pedersen hash.
    pub fn from_leaves(leaves: &[[u8; 32]]) -> Self {
        let tree = EdwardsMaskedMerkleTree::new(PARAMS.clone(), leaves).expect("could not create merkle tree");
        tree.root().clone().into()
    }

    pub const fn size() -> usize {
        32
    }
}

impl From<Fr> for PedersenMerkleRoot {
    fn from(root: Fr) -> PedersenMerkleRoot {
        let root_bytes = root.to_bytes_le().expect("Failed to convert root to bytes");
        assert_eq!(root_bytes.len(), 32);

        let mut buffer = [0u8; 32];
        buffer[..].copy_from_slice(&root_bytes);
        PedersenMerkleRoot(buffer)
    }
}

impl Display for PedersenMerkleRoot {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

/// Calculates the root of the Merkle tree using a Pedersen Hash instantiated with a PRNG and the
/// base layer hashes leaved
pub fn pedersen_merkle_root_hash_with_leaves(hashes: &[[u8; 32]]) -> (Fr, Vec<Fr>) {
    let tree = EdwardsMaskedMerkleTree::new(PARAMS.clone(), hashes).expect("could not create merkle tree");
    (tree.root().clone(), tree.hashed_leaves().to_vec())
}
