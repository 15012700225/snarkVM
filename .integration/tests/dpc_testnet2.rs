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

use snarkvm_algorithms::{
    merkle_tree::MerklePath,
    traits::{CRH, SNARK},
};
use snarkvm_curves::bls12_377::{Fq, Fr};
use snarkvm_dpc::{
    execute_inner_circuit,
    prelude::*,
    testnet2::{execute_outer_circuit, marlin::*, program::NoopProgram, Testnet2Components, TransactionKernel},
    EncryptedRecord,
    InnerCircuit,
    Payload,
    Record,
};
use snarkvm_integration::{ledger::*, memdb::MemDb, storage::*, testnet2::*};
use snarkvm_r1cs::{ConstraintSystem, TestConstraintSystem};
use snarkvm_utilities::{to_bytes_le, FromBytes, ToBytes};

use itertools::Itertools;
use rand::SeedableRng;
use rand_chacha::ChaChaRng;
use snarkvm_fields::ToConstraintField;
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

type L = Ledger<Testnet2Transaction, CommitmentMerkleTreeParameters, MemDb>;

fn testnet2_inner_circuit_id() -> anyhow::Result<Vec<u8>> {
    let dpc = <Testnet2TransactionEngine as DPCScheme<L>>::load(false)?;

    let inner_snark_vk: <<DPC as Testnet2Components>::InnerSNARK as SNARK>::VerifyingKey =
        dpc.inner_snark_parameters.1.clone().into();

    let inner_snark_vk_field_elements = inner_snark_vk.to_field_elements()?;

    let inner_circuit_id =
        <DPC as DPCComponents>::inner_circuit_id_crh().hash_field_elements(&inner_snark_vk_field_elements)?;

    Ok(to_bytes_le![inner_circuit_id]?)
}

/// TODO (howardwu): Update this to the correct inner circuit ID when the final parameters are set.
#[ignore]
#[test]
fn test_testnet2_inner_circuit_sanity_check() {
    let expected_testnet2_inner_circuit_id = vec![
        70, 187, 221, 37, 4, 78, 200, 68, 34, 184, 229, 110, 24, 7, 142, 8, 62, 42, 234, 231, 96, 86, 201, 94, 143,
        197, 248, 117, 32, 218, 44, 219, 109, 191, 72, 112, 157, 76, 212, 91, 7, 14, 32, 183, 79, 1, 194, 0,
    ];
    let candidate_testnet2_inner_circuit_id = testnet2_inner_circuit_id().unwrap();
    assert_eq!(expected_testnet2_inner_circuit_id, candidate_testnet2_inner_circuit_id);
}

#[test]
fn dpc_testnet2_integration_test() {
    let mut rng = ChaChaRng::seed_from_u64(1231275789u64);

    // Generate or load parameters for the ledger, commitment schemes, and CRH.
    let (ledger_parameters, dpc) = setup_or_load_parameters::<_, MemDb>(false, &mut rng);

    // Generate accounts.
    let [genesis_account, recipient, _] = generate_test_accounts::<_>(&mut rng);

    // Create a genesis block.
    let genesis_block = Block {
        header: BlockHeader {
            previous_block_hash: BlockHeaderHash([0u8; 32]),
            merkle_root_hash: MerkleRootHash([0u8; 32]),
            pedersen_merkle_root_hash: PedersenMerkleRootHash([0u8; 32]),
            time: 0,
            difficulty_target: 0x07FF_FFFF_FFFF_FFFF_u64,
            nonce: 0,
            proof: ProofOfSuccinctWork([0u8; 972]),
        },
        transactions: Transactions::new(),
    };

    let ledger = initialize_test_blockchain::<Testnet2Transaction, CommitmentMerkleTreeParameters, MemDb>(
        ledger_parameters,
        genesis_block,
    );

    // Generate dummy input records having as address the genesis address.
    let old_private_keys = vec![genesis_account.private_key.clone(); DPC::NUM_INPUT_RECORDS];

    let mut joint_serial_numbers = vec![];
    let mut old_records = vec![];
    for i in 0..DPC::NUM_INPUT_RECORDS {
        let old_sn_nonce = <DPC as DPCComponents>::serial_number_nonce_crh()
            .hash(&[64u8 + (i as u8); 1])
            .unwrap();
        let old_record = Record::new(
            genesis_account.address.clone(),
            true, // The input record is dummy
            0,
            Payload::default(),
            dpc.noop_program.id(),
            dpc.noop_program.id(),
            old_sn_nonce,
            &mut rng,
        )
        .unwrap();

        let (sn, _) = old_record.to_serial_number(&old_private_keys[i]).unwrap();
        joint_serial_numbers.extend_from_slice(&to_bytes_le![sn].unwrap());

        old_records.push(old_record);
    }

    // Construct new records.

    // Set the new records' program to be the "always-accept" program.
    let mut new_records = vec![];
    for j in 0..DPC::NUM_OUTPUT_RECORDS {
        new_records.push(
            Record::new_full(
                recipient.address.clone(),
                false,
                10,
                Payload::default(),
                dpc.noop_program.id(),
                dpc.noop_program.id(),
                j as u8,
                joint_serial_numbers.clone(),
                &mut rng,
            )
            .unwrap(),
        );
    }

    // Offline execution to generate a DPC transaction kernel.
    let memo = [4u8; 32];
    let transaction_kernel = <Testnet2TransactionEngine as DPCScheme<L>>::execute_offline_phase(
        &dpc,
        &old_private_keys,
        old_records,
        new_records,
        memo,
        &mut rng,
    )
    .unwrap();

    // Generate the program proofs
    let mut program_proofs = vec![];
    for i in 0..DPC::NUM_TOTAL_RECORDS {
        program_proofs.push(
            dpc.noop_program
                .execute(&transaction_kernel.into_local_data(), i as u8, &mut rng)
                .unwrap(),
        );
    }

    let (new_records, transaction) = dpc
        .execute_online_phase(&old_private_keys, transaction_kernel, program_proofs, &ledger, &mut rng)
        .unwrap();

    // Check that the transaction is serialized and deserialized correctly
    let transaction_bytes = to_bytes_le![transaction].unwrap();
    let recovered_transaction = Testnet2Transaction::read_le(&transaction_bytes[..]).unwrap();
    assert_eq!(transaction, recovered_transaction);

    // Check that new_records can be decrypted from the transaction
    {
        let encrypted_records = transaction.encrypted_records();
        let new_account_private_keys = vec![recipient.private_key; DPC::NUM_OUTPUT_RECORDS];

        for ((encrypted_record, private_key), new_record) in
            encrypted_records.iter().zip(new_account_private_keys).zip(new_records)
        {
            let account_view_key = ViewKey::from_private_key(&private_key).unwrap();
            let decrypted_record = encrypted_record.decrypt(&account_view_key).unwrap();
            assert_eq!(decrypted_record, new_record);
        }
    }

    // Craft the block

    let previous_block = ledger.get_latest_block().unwrap();

    let mut transactions = Transactions::new();
    transactions.push(transaction);

    let transaction_ids = transactions.to_transaction_ids().unwrap();

    let mut merkle_root_bytes = [0u8; 32];
    merkle_root_bytes[..].copy_from_slice(&merkle_root(&transaction_ids));

    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs() as i64;

    let header = BlockHeader {
        previous_block_hash: previous_block.header.get_hash(),
        merkle_root_hash: MerkleRootHash(merkle_root_bytes),
        time,
        difficulty_target: previous_block.header.difficulty_target,
        nonce: 0,
        pedersen_merkle_root_hash: PedersenMerkleRootHash([0u8; 32]),
        proof: ProofOfSuccinctWork([0u8; 972]),
    };

    assert!(dpc.verify_transactions(&transactions.0, &ledger));

    let block = Block { header, transactions };

    ledger.insert_and_commit(&block).unwrap();
    assert_eq!(ledger.len(), 2);
}

#[test]
fn test_testnet_2_transaction_kernel_serialization() {
    let mut rng = ChaChaRng::seed_from_u64(1231275789u64);

    // Generate parameters for the ledger, commitment schemes, CRH, and the
    // "always-accept" program.
    let dpc = <Testnet2TransactionEngine as DPCScheme<L>>::load(false).unwrap();

    // Generate metadata and an account for a dummy initial record.
    let test_account = Account::new(&mut rng).unwrap();

    let old_private_keys = vec![test_account.private_key.clone(); DPC::NUM_INPUT_RECORDS];

    // Set the input records for our transaction to be the initial dummy records.
    let mut joint_serial_numbers = vec![];
    let mut old_records = vec![];
    for i in 0..DPC::NUM_INPUT_RECORDS {
        let old_record = Record::new(
            test_account.address.clone(),
            true,
            0,
            Payload::default(),
            dpc.noop_program.id(),
            dpc.noop_program.id(),
            <DPC as DPCComponents>::serial_number_nonce_crh()
                .hash(&[0u8; 1])
                .unwrap(),
            &mut rng,
        )
        .unwrap();

        let (sn, _) = old_record.to_serial_number(&old_private_keys[i]).unwrap();
        joint_serial_numbers.extend_from_slice(&to_bytes_le![sn].unwrap());

        old_records.push(old_record);
    }

    // Construct new records.

    // Set the new record's program to be the "always-accept" program.
    let mut new_records = vec![];
    for j in 0..DPC::NUM_OUTPUT_RECORDS {
        new_records.push(
            Record::new_full(
                test_account.address.clone(),
                false,
                10,
                Payload::default(),
                dpc.noop_program.id(),
                dpc.noop_program.id(),
                j as u8,
                joint_serial_numbers.clone(),
                &mut rng,
            )
            .unwrap(),
        );
    }

    // Generate transaction kernel
    let memo = [0u8; 32];
    let transaction_kernel = <Testnet2TransactionEngine as DPCScheme<L>>::execute_offline_phase(
        &dpc,
        &old_private_keys,
        old_records,
        new_records,
        memo,
        &mut rng,
    )
    .unwrap();

    // Serialize the transaction kernel
    let transaction_kernel_bytes = to_bytes_le![&transaction_kernel].unwrap();
    let recovered_transaction_kernel: <Testnet2TransactionEngine as DPCScheme<L>>::TransactionKernel =
        FromBytes::read_le(&transaction_kernel_bytes[..]).unwrap();
    assert_eq!(transaction_kernel, recovered_transaction_kernel);
}

#[test]
fn test_testnet2_dpc_execute_constraints() {
    let mut rng = ChaChaRng::seed_from_u64(1231275789u64);

    // TODO (howardwu): TEMPORARY - Resolve this inconsistency on import structure with a new model once MerkleParameters are refactored.
    let ledger_parameters = Arc::new(DPC::ledger_merkle_tree_parameters().clone());

    let dpc = <Testnet2TransactionEngine as DPCScheme<L>>::setup(&ledger_parameters, &mut rng).unwrap();

    let alternate_noop_program = NoopProgram::<DPC>::setup(&mut rng).unwrap();

    // Generate metadata and an account for a dummy initial record.
    let dummy_account = Account::new(&mut rng).unwrap();

    let genesis_block = Block {
        header: BlockHeader {
            previous_block_hash: BlockHeaderHash([0u8; 32]),
            merkle_root_hash: MerkleRootHash([0u8; 32]),
            time: 0,
            difficulty_target: 0x07FF_FFFF_FFFF_FFFF_u64,
            nonce: 0,
            pedersen_merkle_root_hash: PedersenMerkleRootHash([0u8; 32]),
            proof: ProofOfSuccinctWork([0u8; 972]),
        },
        transactions: Transactions::new(),
    };

    // Use genesis record, serial number, and memo to initialize the ledger.
    let ledger = initialize_test_blockchain::<Testnet2Transaction, CommitmentMerkleTreeParameters, MemDb>(
        ledger_parameters,
        genesis_block,
    );

    let old_private_keys = vec![dummy_account.private_key; DPC::NUM_INPUT_RECORDS];

    // Set the input records for our transaction to be the initial dummy records.
    let mut joint_serial_numbers = vec![];
    let mut old_records = vec![];
    for i in 0..DPC::NUM_INPUT_RECORDS {
        let old_record = Record::new(
            dummy_account.address.clone(),
            true,
            0,
            Payload::default(),
            alternate_noop_program.id(),
            alternate_noop_program.id(),
            <DPC as DPCComponents>::serial_number_nonce_crh()
                .hash(&[0u8; 1])
                .unwrap(),
            &mut rng,
        )
        .unwrap();

        let (sn, _) = old_record.to_serial_number(&old_private_keys[i]).unwrap();
        joint_serial_numbers.extend_from_slice(&to_bytes_le![sn].unwrap());

        old_records.push(old_record);
    }

    // Create an account for an actual new record.
    let new_account = Account::new(&mut rng).unwrap();

    // Construct new records.

    // Set the new record's program to be the "always-accept" program.
    let mut new_records = vec![];
    for j in 0..DPC::NUM_OUTPUT_RECORDS {
        new_records.push(
            Record::new_full(
                new_account.address.clone(),
                false,
                10,
                Payload::default(),
                dpc.noop_program.id(),
                dpc.noop_program.id(),
                j as u8,
                joint_serial_numbers.clone(),
                &mut rng,
            )
            .unwrap(),
        );
    }

    let memo = [0u8; 32];
    let transaction_kernel = <Testnet2TransactionEngine as DPCScheme<L>>::execute_offline_phase(
        &dpc,
        &old_private_keys,
        old_records,
        new_records,
        memo,
        &mut rng,
    )
    .unwrap();

    // Generate the program proofs

    let mut program_proofs = vec![];
    for i in 0..DPC::NUM_INPUT_RECORDS {
        program_proofs.push(
            alternate_noop_program
                .execute(&transaction_kernel.into_local_data(), i as u8, &mut rng)
                .unwrap(),
        );
    }
    for j in 0..DPC::NUM_OUTPUT_RECORDS {
        program_proofs.push(
            dpc.noop_program
                .execute(
                    &transaction_kernel.into_local_data(),
                    (DPC::NUM_INPUT_RECORDS + j) as u8,
                    &mut rng,
                )
                .unwrap(),
        );
    }

    let TransactionKernel {
        old_records,
        old_serial_numbers,
        old_randomizers: _,

        new_records,
        new_sn_nonce_randomness,
        new_commitments,

        new_records_encryption_randomness,
        new_encrypted_records: _,
        new_encrypted_record_hashes,

        program_commitment,
        program_randomness,
        local_data_merkle_tree,
        local_data_commitment_randomizers,
        value_balance,
        memorandum,
        network_id,
    } = transaction_kernel;

    let local_data_root = local_data_merkle_tree.root();

    // Construct the ledger witnesses
    let ledger_digest = ledger.digest().expect("could not get digest");

    // Generate the ledger membership witnesses
    let mut old_witnesses = Vec::with_capacity(DPC::NUM_INPUT_RECORDS);

    // Compute the ledger membership witness and serial number from the old records.
    for record in old_records.iter() {
        if record.is_dummy() {
            old_witnesses.push(MerklePath::default());
        } else {
            let witness = ledger.prove_cm(&record.commitment()).unwrap();
            old_witnesses.push(witness);
        }
    }

    // Prepare record encryption components used in the inner SNARK
    let mut new_records_encryption_gadget_components = Vec::with_capacity(DPC::NUM_OUTPUT_RECORDS);
    for (record, ciphertext_randomness) in new_records.iter().zip_eq(&new_records_encryption_randomness) {
        let record_encryption_gadget_components =
            EncryptedRecord::prepare_encryption_gadget_components(&record, ciphertext_randomness).unwrap();

        new_records_encryption_gadget_components.push(record_encryption_gadget_components);
    }

    //////////////////////////////////////////////////////////////////////////
    // Check that the core check constraint system was satisfied.
    let mut inner_circuit_cs = TestConstraintSystem::<Fr>::new();

    execute_inner_circuit(
        &mut inner_circuit_cs.ns(|| "Inner circuit"),
        ledger.parameters(),
        &ledger_digest,
        &old_records,
        &old_witnesses,
        &old_private_keys,
        &old_serial_numbers,
        &new_records,
        &new_sn_nonce_randomness,
        &new_commitments,
        &new_records_encryption_randomness,
        &new_records_encryption_gadget_components,
        &new_encrypted_record_hashes,
        &program_commitment,
        &program_randomness,
        &local_data_root,
        &local_data_commitment_randomizers,
        &memo,
        value_balance,
        network_id,
    )
    .unwrap();

    if !inner_circuit_cs.is_satisfied() {
        println!("=========================================================");
        println!(
            "Inner circuit num constraints: {:?}",
            inner_circuit_cs.num_constraints()
        );
        println!("Unsatisfied constraints:");
        println!("{}", inner_circuit_cs.which_is_unsatisfied().unwrap());
        println!("=========================================================");
    }

    {
        println!("=========================================================");
        let num_constraints = inner_circuit_cs.num_constraints();
        println!("Inner circuit num constraints: {:?}", num_constraints);
        assert_eq!(417683, num_constraints);
        println!("=========================================================");
    }

    assert!(inner_circuit_cs.is_satisfied());

    // Generate inner snark parameters and proof for verification in the outer snark
    let inner_snark_parameters =
        <DPC as Testnet2Components>::InnerSNARK::setup(&InnerCircuit::blank(ledger.parameters()), &mut rng).unwrap();

    let inner_snark_vk: <<DPC as Testnet2Components>::InnerSNARK as SNARK>::VerifyingKey =
        inner_snark_parameters.1.clone().into();

    let inner_snark_vk_field_elements = inner_snark_vk.to_field_elements().unwrap();

    let inner_circuit_id = <DPC as DPCComponents>::inner_circuit_id_crh()
        .hash_field_elements(&inner_snark_vk_field_elements)
        .unwrap();

    let inner_snark_proof = <DPC as Testnet2Components>::InnerSNARK::prove(
        &inner_snark_parameters.0,
        &InnerCircuit::new(
            ledger.parameters().clone(),
            ledger_digest,
            old_records,
            old_witnesses,
            old_private_keys,
            old_serial_numbers.clone(),
            new_records,
            new_sn_nonce_randomness,
            new_commitments.clone(),
            new_records_encryption_randomness,
            new_records_encryption_gadget_components,
            new_encrypted_record_hashes.clone(),
            program_commitment,
            program_randomness,
            local_data_root,
            local_data_commitment_randomizers,
            memo,
            value_balance,
            network_id,
        ),
        &mut rng,
    )
    .unwrap();

    // Check that the proof check constraint system was satisfied.
    let mut outer_circuit_cs = TestConstraintSystem::<Fq>::new();

    execute_outer_circuit::<DPC, _>(
        &mut outer_circuit_cs.ns(|| "Outer circuit"),
        ledger.parameters(),
        &ledger_digest,
        &old_serial_numbers,
        &new_commitments,
        &new_encrypted_record_hashes,
        &memorandum,
        value_balance,
        network_id,
        &inner_snark_vk,
        &inner_snark_proof,
        &program_proofs,
        &program_commitment,
        &program_randomness,
        &local_data_root,
        &inner_circuit_id,
    )
    .unwrap();

    if !outer_circuit_cs.is_satisfied() {
        println!("=========================================================");
        println!(
            "Outer circuit num constraints: {:?}",
            outer_circuit_cs.num_constraints()
        );
        println!("Unsatisfied constraints:");
        println!("{}", outer_circuit_cs.which_is_unsatisfied().unwrap());
        println!("=========================================================");
    }

    {
        println!("=========================================================");
        let num_constraints = outer_circuit_cs.num_constraints();
        println!("Outer circuit num constraints: {:?}", num_constraints);
        assert_eq!(834532, num_constraints);
        println!("=========================================================");
    }

    assert!(outer_circuit_cs.is_satisfied());
}
