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

use crate::poseidon::{grain_lfsr::PoseidonGrainLFSR, PoseidonParameters};
use snarkvm_fields::*;
use snarkvm_utilities::*;

/// A field with Poseidon parameters associated
pub trait PoseidonDefaultParametersField: PrimeField {
    /// Obtain the default Poseidon parameters for this rate and for this prime field,
    /// with a specific optimization goal.
    fn get_default_parameters(rate: usize, optimized_for_constraints: bool) -> Option<PoseidonParameters<Self>>;
}

/// Internal function that uses the `PoseidonDefaultParameters` to compute the Poseidon parameters.
pub fn get_default_poseidon_parameters_internal<F: PrimeField, P: PoseidonDefaultParameters>(
    rate: usize,
    optimized_for_weights: bool,
) -> Option<PoseidonParameters<F>> {
    let params_set = if !optimized_for_weights {
        P::PARAMS_OPT_FOR_CONSTRAINTS
    } else {
        P::PARAMS_OPT_FOR_WEIGHTS
    };

    for param in params_set.iter() {
        if param[0] == rate {
            let (ark, mds) = find_poseidon_ark_and_mds::<F>(
                P::MODULUS_BITS as u64,
                rate,
                param[2] as u64,
                param[3] as u64,
                param[4] as u64,
            );

            return Some(PoseidonParameters {
                full_rounds: param[2],
                partial_rounds: param[3],
                alpha: param[1] as u64,
                ark,
                mds,
                rate: param[0],
                capacity: 1,
            });
        }
    }

    None
}

/// Internal function that computes the ark and mds from the Poseidon Grain LFSR.
pub fn find_poseidon_ark_and_mds<F: PrimeField>(
    prime_bits: u64,
    rate: usize,
    full_rounds: u64,
    partial_rounds: u64,
    skip_matrices: u64,
) -> (Vec<Vec<F>>, Vec<Vec<F>>) {
    let mut lfsr = PoseidonGrainLFSR::new(false, prime_bits, (rate + 1) as u64, full_rounds, partial_rounds);

    let mut ark = Vec::<Vec<F>>::new();
    for _ in 0..(full_rounds + partial_rounds) {
        ark.push(lfsr.get_field_elements_rejection_sampling(rate + 1));
    }

    let mut mds = Vec::<Vec<F>>::new();
    mds.resize(rate + 1, vec![F::zero(); rate + 1]);
    for _ in 0..skip_matrices {
        let _ = lfsr.get_field_elements_mod_p::<F>(2 * (rate + 1));
    }

    // a qualifying matrix must satisfy the following requirements
    // - there is no duplication among the elements in x or y
    // - there is no i and j such that x[i] + y[j] = p
    // - the resultant MDS passes all the three tests

    let xs = lfsr.get_field_elements_mod_p::<F>(rate + 1);
    let ys = lfsr.get_field_elements_mod_p::<F>(rate + 1);

    for i in 0..(rate + 1) {
        for j in 0..(rate + 1) {
            mds[i][j] = (xs[i] + &ys[j]).inverse().unwrap();
        }
    }

    (ark, mds)
}

macro_rules! impl_poseidon_default_parameters_field {
    ($field: ident, $params: ident) => {
        impl<P: $params + PoseidonDefaultParameters> PoseidonDefaultParametersField for $field<P> {
            fn get_default_parameters(rate: usize, optimized_for_weights: bool) -> Option<PoseidonParameters<Self>> {
                get_default_poseidon_parameters_internal::<Self, P>(rate, optimized_for_weights)
            }
        }
    };
}

impl_poseidon_default_parameters_field!(Fp256, Fp256Parameters);
impl_poseidon_default_parameters_field!(Fp320, Fp320Parameters);
impl_poseidon_default_parameters_field!(Fp384, Fp384Parameters);
impl_poseidon_default_parameters_field!(Fp768, Fp768Parameters);
impl_poseidon_default_parameters_field!(Fp832, Fp832Parameters);

#[cfg(test)]
mod test {
    use crate::PoseidonDefaultParametersField;
    use snarkvm_curves::bls12_377::Fr as TestFr;
    use snarkvm_utilities::{str::FromStr, *};

    #[test]
    fn bls12_377_fr_poseidon_default_parameters_test() {
        // constraints
        let constraints_rate_2 = TestFr::get_default_parameters(2, false).unwrap();
        assert_eq!(
            constraints_rate_2.ark[0][0],
            TestFr::from_str("1370773116404421539888881648821194629032979299946048429076387284005101684675").unwrap()
        );
        assert_eq!(
            constraints_rate_2.mds[0][0],
            TestFr::from_str("6093452032963406658309134825240609333033222270199073508119142384975416392638").unwrap()
        );

        let constraints_rate_3 = TestFr::get_default_parameters(3, false).unwrap();
        assert_eq!(
            constraints_rate_3.ark[0][0],
            TestFr::from_str("2735315691567496447407171152736750055976064076954958868732156315289790632296").unwrap()
        );
        assert_eq!(
            constraints_rate_3.mds[0][0],
            TestFr::from_str("4163779913938300929692849383966514284858040938385522126460051994426579147809").unwrap()
        );

        let constraints_rate_4 = TestFr::get_default_parameters(4, false).unwrap();
        assert_eq!(
            constraints_rate_4.ark[0][0],
            TestFr::from_str("1938618153915392443680844598029810201246194507135996901458264098669274389515").unwrap()
        );
        assert_eq!(
            constraints_rate_4.mds[0][0],
            TestFr::from_str("8329930521539134039137773392305942487936411634375145971571337914339858953494").unwrap()
        );

        let constraints_rate_5 = TestFr::get_default_parameters(5, false).unwrap();
        assert_eq!(
            constraints_rate_5.ark[0][0],
            TestFr::from_str("1813936142909156450253609849912578699088995753219507490338048666753865510158").unwrap()
        );
        assert_eq!(
            constraints_rate_5.mds[0][0],
            TestFr::from_str("2818272963400663000142153607607282295699644585739987409189194178234128477324").unwrap()
        );

        let constraints_rate_6 = TestFr::get_default_parameters(6, false).unwrap();
        assert_eq!(
            constraints_rate_6.ark[0][0],
            TestFr::from_str("445601323772778241019796483204016315895174980479504798033553904152974044363").unwrap()
        );
        assert_eq!(
            constraints_rate_6.mds[0][0],
            TestFr::from_str("7928691668574423590377019133144443220009376833944986026812291791207365073467").unwrap()
        );

        let constraints_rate_7 = TestFr::get_default_parameters(7, false).unwrap();
        assert_eq!(
            constraints_rate_7.ark[0][0],
            TestFr::from_str("5479835938894296979622951496762907006619403688067952535748490445328204262522").unwrap()
        );
        assert_eq!(
            constraints_rate_7.mds[0][0],
            TestFr::from_str("2044738373754673904510791010534193940992981049813410588199717111822742849411").unwrap()
        );

        let constraints_rate_8 = TestFr::get_default_parameters(8, false).unwrap();
        assert_eq!(
            constraints_rate_8.ark[0][0],
            TestFr::from_str("2806882019829952968543507592167502510188638053153774646465991640201889135551").unwrap()
        );
        assert_eq!(
            constraints_rate_8.mds[0][0],
            TestFr::from_str("3195939821470342407043866187037485190412960074203265869296316033794290707270").unwrap()
        );

        // weights
        let weights_rate_2 = TestFr::get_default_parameters(2, true).unwrap();
        assert_eq!(
            weights_rate_2.ark[0][0],
            TestFr::from_str("1437553550906659983785289949566121426573444168096671364956005111200187784882").unwrap()
        );
        assert_eq!(
            weights_rate_2.mds[0][0],
            TestFr::from_str("4948200626912352237754042596065430013507774274004957341305683445394766579").unwrap()
        );

        let weights_rate_3 = TestFr::get_default_parameters(3, true).unwrap();
        assert_eq!(
            weights_rate_3.ark[0][0],
            TestFr::from_str("2389141789821188973542200621423955168213098224545396543007181312070011262708").unwrap()
        );
        assert_eq!(
            weights_rate_3.mds[0][0],
            TestFr::from_str("423353182496175764060161596636602791590914187146909704631803120075886050962").unwrap()
        );

        let weights_rate_4 = TestFr::get_default_parameters(4, true).unwrap();
        assert_eq!(
            weights_rate_4.ark[0][0],
            TestFr::from_str("339665592737921178987860673780531325874373991183648566680235308535235434155").unwrap()
        );
        assert_eq!(
            weights_rate_4.mds[0][0],
            TestFr::from_str("5556224500064780444200287711148584033360859502164827797696333140428735465665").unwrap()
        );

        let weights_rate_5 = TestFr::get_default_parameters(5, true).unwrap();
        assert_eq!(
            weights_rate_5.ark[0][0],
            TestFr::from_str("6657421650565674088522419767333860567475243617250708801117704713863238267580").unwrap()
        );
        assert_eq!(
            weights_rate_5.mds[0][0],
            TestFr::from_str("5871197260273268717721824310974992043863315749361747100112666675151011947534").unwrap()
        );

        let weights_rate_6 = TestFr::get_default_parameters(6, true).unwrap();
        assert_eq!(
            weights_rate_6.ark[0][0],
            TestFr::from_str("1786977981326127053469902924887139723627870007983183229969971478961095160022").unwrap()
        );
        assert_eq!(
            weights_rate_6.mds[0][0],
            TestFr::from_str("7418852714447187929762593123308060895010328396277776662118660542911937941223").unwrap()
        );

        let weights_rate_7 = TestFr::get_default_parameters(7, true).unwrap();
        assert_eq!(
            weights_rate_7.ark[0][0],
            TestFr::from_str("7030082714176479058338944551176555018598671376667849904053365110094189638249").unwrap()
        );
        assert_eq!(
            weights_rate_7.mds[0][0],
            TestFr::from_str("989151420287259756087222590828952829034250078776206542075927644905827881485").unwrap()
        );

        let weights_rate_8 = TestFr::get_default_parameters(8, true).unwrap();
        assert_eq!(
            weights_rate_8.ark[0][0],
            TestFr::from_str("4856206629629142966731182650528313310315690501799963950270209620500247150005").unwrap()
        );
        assert_eq!(
            weights_rate_8.mds[0][0],
            TestFr::from_str("2147366300731764725485276624951065964179916161151487340006324219449683366351").unwrap()
        );
    }
}
