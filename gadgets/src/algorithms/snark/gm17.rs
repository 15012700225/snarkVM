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

use std::{borrow::Borrow, marker::PhantomData};

use snarkvm_algorithms::snark::gm17::{Proof, VerifyingKey, GM17};
use snarkvm_curves::traits::{AffineCurve, PairingEngine};
use snarkvm_fields::ToConstraintField;
use snarkvm_r1cs::{errors::SynthesisError, ConstraintSynthesizer, ConstraintSystem};

use crate::{
    bits::{Boolean, ToBitsBEGadget},
    traits::{
        algorithms::SNARKVerifierGadget,
        alloc::AllocGadget,
        curves::{GroupGadget, PairingGadget},
        eq::EqGadget,
        fields::FieldGadget,
    },
    FpGadget,
    PrepareGadget,
    ToConstraintFieldGadget,
};

#[derive(Derivative)]
#[derivative(Clone(bound = "P::G1Gadget: Clone, P::G2Gadget: Clone"))]
pub struct GM17ProofGadget<Pairing: PairingEngine, P: PairingGadget<Pairing>> {
    pub a: P::G1Gadget,
    pub b: P::G2Gadget,
    pub c: P::G1Gadget,
}

#[derive(Derivative)]
#[derivative(Clone(bound = "P::G1Gadget: Clone, P::GTGadget: Clone, P::G1PreparedGadget: Clone, \
                            P::G2PreparedGadget: Clone, "))]
pub struct GM17VerifyingKeyGadget<Pairing: PairingEngine, P: PairingGadget<Pairing>> {
    pub h_g2: P::G2Gadget,
    pub g_alpha_g1: P::G1Gadget,
    pub h_beta_g2: P::G2Gadget,
    pub g_gamma_g1: P::G1Gadget,
    pub h_gamma_g2: P::G2Gadget,
    pub query: Vec<P::G1Gadget>,
}

impl<Pairing: PairingEngine, P: PairingGadget<Pairing>>
    PrepareGadget<GM17PreparedVerifyingKeyGadget<Pairing, P>, Pairing::Fq> for GM17VerifyingKeyGadget<Pairing, P>
{
    fn prepare<CS: ConstraintSystem<Pairing::Fq>>(
        &self,
        mut cs: CS,
    ) -> Result<GM17PreparedVerifyingKeyGadget<Pairing, P>, SynthesisError> {
        let mut cs = cs.ns(|| "Preparing verifying key");
        let g_alpha_pc = P::prepare_g1(&mut cs.ns(|| "Prepare g_alpha_g1"), self.g_alpha_g1.clone())?;
        let h_beta_pc = P::prepare_g2(&mut cs.ns(|| "Prepare h_beta_g2"), self.h_beta_g2.clone())?;
        let g_gamma_pc = P::prepare_g1(&mut cs.ns(|| "Prepare g_gamma_pc"), self.g_gamma_g1.clone())?;
        let h_gamma_pc = P::prepare_g2(&mut cs.ns(|| "Prepare h_gamma_pc"), self.h_gamma_g2.clone())?;
        let h_pc = P::prepare_g2(&mut cs.ns(|| "Prepare h_pc"), self.h_g2.clone())?;
        Ok(GM17PreparedVerifyingKeyGadget {
            g_alpha: self.g_alpha_g1.clone(),
            h_beta: self.h_beta_g2.clone(),
            g_alpha_pc,
            h_beta_pc,
            g_gamma_pc,
            h_gamma_pc,
            h_pc,
            query: self.query.clone(),
        })
    }
}

#[derive(Derivative)]
#[derivative(Clone(bound = "P::G1Gadget: Clone, P::GTGadget: Clone, P::G1PreparedGadget: Clone, \
                            P::G2PreparedGadget: Clone, "))]
pub struct GM17PreparedVerifyingKeyGadget<Pairing: PairingEngine, P: PairingGadget<Pairing>> {
    pub g_alpha: P::G1Gadget,
    pub h_beta: P::G2Gadget,
    pub g_alpha_pc: P::G1PreparedGadget,
    pub h_beta_pc: P::G2PreparedGadget,
    pub g_gamma_pc: P::G1PreparedGadget,
    pub h_gamma_pc: P::G2PreparedGadget,
    pub h_pc: P::G2PreparedGadget,
    pub query: Vec<P::G1Gadget>,
}

pub struct GM17VerifierGadget<Pairing: PairingEngine, P: PairingGadget<Pairing>> {
    _pairing_engine: PhantomData<Pairing>,
    _pairing_gadget: PhantomData<P>,
}

impl<Pairing: PairingEngine, P: PairingGadget<Pairing>, V: ToConstraintField<Pairing::Fr>, C>
    SNARKVerifierGadget<Pairing::Fr, Pairing::Fq, GM17<Pairing, C, V>> for GM17VerifierGadget<Pairing, P>
where
    C: ConstraintSynthesizer<Pairing::Fr>,
{
    type Input = Vec<Boolean>;
    type PreparedVerificationKeyGadget = GM17PreparedVerifyingKeyGadget<Pairing, P>;
    type ProofGadget = GM17ProofGadget<Pairing, P>;
    type VerificationKeyGadget = GM17VerifyingKeyGadget<Pairing, P>;

    fn prepared_check_verify<CS: ConstraintSystem<Pairing::Fq>, I: Iterator<Item = Self::Input>>(
        mut cs: CS,
        pvk: &Self::PreparedVerificationKeyGadget,
        mut public_inputs: I,
        proof: &Self::ProofGadget,
    ) -> Result<(), SynthesisError> {
        // e(A*G^{alpha}, B*H^{beta}) = e(G^{alpha}, H^{beta}) * e(G^{psi}, H^{gamma}) *
        // e(C, H) where psi = \sum_{i=0}^l input_i pvk.query[i]

        let g_psi = {
            let mut cs = cs.ns(|| "Process input");
            let mut g_psi = pvk.query[0].clone();
            let mut input_len = 1;
            for (i, (input, b)) in public_inputs.by_ref().zip(pvk.query.iter().skip(1)).enumerate() {
                let input_bits = input.to_bits_be(cs.ns(|| format!("Input {}", i)))?;
                g_psi = b.mul_bits(cs.ns(|| format!("Mul {}", i)), &g_psi, input_bits.into_iter())?;
                input_len += 1;
            }
            // Check that the input and the query in the verification are of the
            // same length.
            assert!(input_len == pvk.query.len() && public_inputs.next().is_none());
            g_psi
        };

        let mut test1_a_g_alpha = proof.a.add(cs.ns(|| "A * G^{alpha}"), &pvk.g_alpha)?;
        let test1_b_h_beta = proof.b.add(cs.ns(|| "B * H^{beta}"), &pvk.h_beta)?;

        let test1_exp = {
            test1_a_g_alpha = test1_a_g_alpha.negate(cs.ns(|| "neg 1"))?;
            let test1_a_g_alpha_prep = P::prepare_g1(cs.ns(|| "First prep"), test1_a_g_alpha)?;
            let test1_b_h_beta_prep = P::prepare_g2(cs.ns(|| "Second prep"), test1_b_h_beta)?;

            let g_psi_prep = P::prepare_g1(cs.ns(|| "Third prep"), g_psi)?;

            let c_prep = P::prepare_g1(cs.ns(|| "Fourth prep"), proof.c.clone())?;

            P::miller_loop(
                cs.ns(|| "Miller loop 1"),
                &[test1_a_g_alpha_prep, g_psi_prep, c_prep, pvk.g_alpha_pc.clone()],
                &[
                    test1_b_h_beta_prep,
                    pvk.h_gamma_pc.clone(),
                    pvk.h_pc.clone(),
                    pvk.h_beta_pc.clone(),
                ],
            )?
        };

        let test1 = P::final_exponentiation(cs.ns(|| "Final Exp 1"), &test1_exp).unwrap();

        // e(A, H^{gamma}) = e(G^{gamma}, B)
        let test2_exp = {
            let a_prep = P::prepare_g1(cs.ns(|| "Fifth prep"), proof.a.clone())?;
            // pvk.h_gamma_pc
            //&pvk.g_gamma_pc
            let proof_b = proof.b.negate(cs.ns(|| "Negate b"))?;
            let b_prep = P::prepare_g2(cs.ns(|| "Sixth prep"), proof_b)?;
            P::miller_loop(cs.ns(|| "Miller loop 4"), &[a_prep, pvk.g_gamma_pc.clone()], &[
                pvk.h_gamma_pc.clone(),
                b_prep,
            ])?
        };
        let test2 = P::final_exponentiation(cs.ns(|| "Final Exp 2"), &test2_exp)?;

        let one = P::GTGadget::one(cs.ns(|| "GT One"))?;
        test1.enforce_equal(cs.ns(|| "Test 1"), &one)?;
        test2.enforce_equal(cs.ns(|| "Test 2"), &one)?;
        Ok(())
    }

    fn check_verify<'a, CS: ConstraintSystem<Pairing::Fq>, I: Iterator<Item = Self::Input>>(
        mut cs: CS,
        vk: &Self::VerificationKeyGadget,
        input: I,
        proof: &Self::ProofGadget,
    ) -> Result<(), SynthesisError> {
        let pvk = vk.prepare(cs.ns(|| "Prepare vk"))?;
        <Self as SNARKVerifierGadget<Pairing::Fr, Pairing::Fq, GM17<Pairing, C, V>>>::prepared_check_verify(
            cs.ns(|| "prepared_verification"),
            &pvk,
            input,
            proof,
        )
    }
}

impl<Pairing: PairingEngine, P: PairingGadget<Pairing>> AllocGadget<VerifyingKey<Pairing>, Pairing::Fq>
    for GM17VerifyingKeyGadget<Pairing, P>
{
    #[inline]
    fn alloc<
        Fn: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<VerifyingKey<Pairing>>,
        CS: ConstraintSystem<Pairing::Fq>,
    >(
        mut cs: CS,
        value_gen: Fn,
    ) -> Result<Self, SynthesisError> {
        value_gen().and_then(|vk| {
            let VerifyingKey {
                h_g2,
                g_alpha_g1,
                h_beta_g2,
                g_gamma_g1,
                h_gamma_g2,
                query,
            } = vk.borrow().clone();
            let h_g2 = P::G2Gadget::alloc(cs.ns(|| "h_g2"), || Ok(h_g2.into_projective()))?;
            let g_alpha_g1 = P::G1Gadget::alloc(cs.ns(|| "g_alpha"), || Ok(g_alpha_g1.into_projective()))?;
            let h_beta_g2 = P::G2Gadget::alloc(cs.ns(|| "h_beta"), || Ok(h_beta_g2.into_projective()))?;
            let g_gamma_g1 = P::G1Gadget::alloc(cs.ns(|| "g_gamma_g1"), || Ok(g_gamma_g1.into_projective()))?;
            let h_gamma_g2 = P::G2Gadget::alloc(cs.ns(|| "h_gamma_g2"), || Ok(h_gamma_g2.into_projective()))?;

            let query = query
                .into_iter()
                .enumerate()
                .map(|(i, query_i)| {
                    P::G1Gadget::alloc(cs.ns(|| format!("query_{}", i)), || Ok(query_i.into_projective()))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<Result<_, _>>()?;
            Ok(Self {
                h_g2,
                g_alpha_g1,
                h_beta_g2,
                g_gamma_g1,
                h_gamma_g2,
                query,
            })
        })
    }

    #[inline]
    fn alloc_input<
        Fn: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<VerifyingKey<Pairing>>,
        CS: ConstraintSystem<Pairing::Fq>,
    >(
        mut cs: CS,
        value_gen: Fn,
    ) -> Result<Self, SynthesisError> {
        value_gen().and_then(|vk| {
            let VerifyingKey {
                h_g2,
                g_alpha_g1,
                h_beta_g2,
                g_gamma_g1,
                h_gamma_g2,
                query,
            } = vk.borrow().clone();
            let h_g2 = P::G2Gadget::alloc_input(cs.ns(|| "h_g2"), || Ok(h_g2.into_projective()))?;
            let g_alpha_g1 = P::G1Gadget::alloc_input(cs.ns(|| "g_alpha"), || Ok(g_alpha_g1.into_projective()))?;
            let h_beta_g2 = P::G2Gadget::alloc_input(cs.ns(|| "h_beta"), || Ok(h_beta_g2.into_projective()))?;
            let g_gamma_g1 = P::G1Gadget::alloc_input(cs.ns(|| "g_gamma_g1"), || Ok(g_gamma_g1.into_projective()))?;
            let h_gamma_g2 = P::G2Gadget::alloc_input(cs.ns(|| "h_gamma_g2"), || Ok(h_gamma_g2.into_projective()))?;

            let query = query
                .into_iter()
                .enumerate()
                .map(|(i, query_i)| {
                    P::G1Gadget::alloc_input(cs.ns(|| format!("query_{}", i)), || Ok(query_i.into_projective()))
                })
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<Result<_, _>>()?;
            Ok(Self {
                h_g2,
                g_alpha_g1,
                h_beta_g2,
                g_gamma_g1,
                h_gamma_g2,
                query,
            })
        })
    }
}

impl<Pairing: PairingEngine, P: PairingGadget<Pairing>> ToConstraintFieldGadget<Pairing::Fq>
    for GM17VerifyingKeyGadget<Pairing, P>
{
    fn to_constraint_field<CS: ConstraintSystem<Pairing::Fq>>(
        &self,
        mut cs: CS,
    ) -> Result<Vec<FpGadget<Pairing::Fq>>, SynthesisError> {
        let mut res = Vec::new();
        res.append(&mut self.h_g2.to_constraint_field(cs.ns(|| "h_g2"))?);
        res.append(&mut self.g_alpha_g1.to_constraint_field(cs.ns(|| "g_alpha_g1"))?);
        res.append(&mut self.h_beta_g2.to_constraint_field(cs.ns(|| "h_beta_g2"))?);
        res.append(&mut self.g_gamma_g1.to_constraint_field(cs.ns(|| "g_gamma_g1"))?);
        res.append(&mut self.h_gamma_g2.to_constraint_field(cs.ns(|| "h_gamma_g2"))?);
        for (i, query_elem) in self.query.iter().enumerate() {
            res.append(&mut query_elem.to_constraint_field(cs.ns(|| format!("query_{}", i)))?);
        }

        Ok(res)
    }
}

impl<Pairing: PairingEngine, P: PairingGadget<Pairing>> AllocGadget<Proof<Pairing>, Pairing::Fq>
    for GM17ProofGadget<Pairing, P>
{
    #[inline]
    fn alloc<
        Fn: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<Proof<Pairing>>,
        CS: ConstraintSystem<Pairing::Fq>,
    >(
        mut cs: CS,
        value_gen: Fn,
    ) -> Result<Self, SynthesisError> {
        value_gen().and_then(|proof| {
            let Proof { a, b, c, .. } = proof.borrow().clone();
            let a = P::G1Gadget::alloc_checked(cs.ns(|| "a"), || Ok(a.into_projective()))?;
            let b = P::G2Gadget::alloc_checked(cs.ns(|| "b"), || Ok(b.into_projective()))?;
            let c = P::G1Gadget::alloc_checked(cs.ns(|| "c"), || Ok(c.into_projective()))?;
            Ok(Self { a, b, c })
        })
    }

    #[inline]
    fn alloc_input<
        Fn: FnOnce() -> Result<T, SynthesisError>,
        T: Borrow<Proof<Pairing>>,
        CS: ConstraintSystem<Pairing::Fq>,
    >(
        mut cs: CS,
        value_gen: Fn,
    ) -> Result<Self, SynthesisError> {
        value_gen().and_then(|proof| {
            let Proof { a, b, c, .. } = proof.borrow().clone();
            // We don't need to check here because the prime order check can be performed
            // in plain.
            let a = P::G1Gadget::alloc_input(cs.ns(|| "a"), || Ok(a.into_projective()))?;
            let b = P::G2Gadget::alloc_input(cs.ns(|| "b"), || Ok(b.into_projective()))?;
            let c = P::G1Gadget::alloc_input(cs.ns(|| "c"), || Ok(c.into_projective()))?;
            Ok(Self { a, b, c })
        })
    }
}
