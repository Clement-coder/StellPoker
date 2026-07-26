//! UltraHonk relation accumulation for verifier-side sumcheck final check.
//! This matches co-noir's verifier ordering and formulas.

use crate::field::Fr;
use crate::types::{RelationParameters, Wire, NUMBER_OF_SUBRELATIONS};

#[inline(always)]
fn wire(vals: &[Fr], w: Wire) -> Fr {
    vals[w.index()]
}

#[inline(always)]
fn neg_half() -> Fr {
    Fr::from_hex("0x183227397098d014dc2822db40c0ac2e9419f4243cdcb848a1f0fac9f8000000")
}

#[inline(always)]
fn curve_b() -> Fr {
    Fr::zero() - Fr::from_u64(17)
}

#[inline(always)]
fn limb_size() -> Fr {
    Fr::from_hex("0x100000000000000000")
}

#[inline(always)]
fn sublimb_shift() -> Fr {
    Fr::from_u64(1 << 14)
}

#[inline(always)]
fn internal_matrix_diagonal() -> [Fr; 4] {
    [
        Fr::from_hex("0x10dc6e9c006ea38b04b1e03b4bd9490c0d03f98929ca1d7fb56821fd19d3b6e7"),
        Fr::from_hex("0x0c28145b6a44df3e0149b3d0a30b3bb599df9756d4dd9b84a86b38cfb45a740b"),
        Fr::from_hex("0x00544b8338791518b2c7645a50392798b21f75bb60e3596170067d00141cac15"),
        Fr::from_hex("0x222c01175718386f2e2e82eb122789e352e105a3b8fa852613bc534433ee428b"),
    ]
}

// 0..=1
fn accumulate_arithmetic_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let w_l = wire(p, Wire::Wl);
    let w_r = wire(p, Wire::Wr);
    let w_o = wire(p, Wire::Wo);
    let w_4 = wire(p, Wire::W4);
    let w_4_shift = wire(p, Wire::W4Shift);
    let q_m = wire(p, Wire::Qm);
    let q_l = wire(p, Wire::Ql);
    let q_r = wire(p, Wire::Qr);
    let q_o = wire(p, Wire::Qo);
    let q_4 = wire(p, Wire::Q4);
    let q_c = wire(p, Wire::Qc);
    let q_arith = wire(p, Wire::QArith);
    let w_l_shift = wire(p, Wire::WlShift);

    let mut r0 = (q_arith - Fr::from_u64(3)) * (q_m * w_r * w_l) * neg_half();
    r0 = r0 + q_l * w_l + q_r * w_r + q_o * w_o + q_4 * w_4 + q_c;
    r0 = r0 + (q_arith - Fr::one()) * w_4_shift;
    r0 = r0 * q_arith * domain_sep;
    evals[0] = r0;

    let mut r1 = w_l + w_4 - w_l_shift + q_m;
    r1 = r1 * (q_arith - Fr::from_u64(2));
    r1 = r1 * (q_arith - Fr::one());
    r1 = r1 * q_arith * domain_sep;
    evals[1] = r1;
}

// 2..=3
fn accumulate_permutation_relation(
    p: &[Fr],
    rp: &RelationParameters,
    evals: &mut [Fr],
    domain_sep: Fr,
) {
    let w_1 = wire(p, Wire::Wl);
    let w_2 = wire(p, Wire::Wr);
    let w_3 = wire(p, Wire::Wo);
    let w_4 = wire(p, Wire::W4);
    let id_1 = wire(p, Wire::Id1);
    let id_2 = wire(p, Wire::Id2);
    let id_3 = wire(p, Wire::Id3);
    let id_4 = wire(p, Wire::Id4);
    let sigma_1 = wire(p, Wire::Sigma1);
    let sigma_2 = wire(p, Wire::Sigma2);
    let sigma_3 = wire(p, Wire::Sigma3);
    let sigma_4 = wire(p, Wire::Sigma4);
    let z_perm = wire(p, Wire::ZPerm);
    let z_perm_shift = wire(p, Wire::ZPermShift);
    let lagrange_first = wire(p, Wire::LagrangeFirst);
    let lagrange_last = wire(p, Wire::LagrangeLast);

    let w1_plus_gamma = w_1 + rp.gamma;
    let w2_plus_gamma = w_2 + rp.gamma;
    let w3_plus_gamma = w_3 + rp.gamma;
    let w4_plus_gamma = w_4 + rp.gamma;

    let mut numerator = (id_1 * rp.beta + w1_plus_gamma) * domain_sep;
    numerator = numerator * (id_2 * rp.beta + w2_plus_gamma);
    numerator = numerator * (id_3 * rp.beta + w3_plus_gamma);
    numerator = numerator * (id_4 * rp.beta + w4_plus_gamma);

    let mut denominator = (sigma_1 * rp.beta + w1_plus_gamma) * domain_sep;
    denominator = denominator * (sigma_2 * rp.beta + w2_plus_gamma);
    denominator = denominator * (sigma_3 * rp.beta + w3_plus_gamma);
    denominator = denominator * (sigma_4 * rp.beta + w4_plus_gamma);

    let public_input_term = lagrange_last * rp.public_inputs_delta + z_perm_shift;
    evals[2] = (z_perm + lagrange_first) * numerator - public_input_term * denominator;
    evals[3] = lagrange_last * z_perm_shift * domain_sep;
}

// 4..=6
fn accumulate_log_derivative_lookup_relation(
    p: &[Fr],
    rp: &RelationParameters,
    evals: &mut [Fr],
    domain_sep: Fr,
) {
    let inverses = wire(p, Wire::LookupInverses);
    let read_counts = wire(p, Wire::LookupReadCounts);
    let read_selector = wire(p, Wire::QLookup);
    let read_tag = wire(p, Wire::LookupReadTags);

    let inverse_exists = (Fr::zero() - read_tag * read_selector) + read_tag + read_selector;

    let read_term = {
        let derived_table_entry_1 =
            wire(p, Wire::Wl) + rp.gamma + wire(p, Wire::Qr) * wire(p, Wire::WlShift);
        let derived_table_entry_2 = wire(p, Wire::Qm) * wire(p, Wire::WrShift) + wire(p, Wire::Wr);
        let derived_table_entry_3 = wire(p, Wire::Qc) * wire(p, Wire::WoShift) + wire(p, Wire::Wo);
        derived_table_entry_1
            + derived_table_entry_2 * rp.eta
            + derived_table_entry_3 * rp.eta_two
            + wire(p, Wire::Qo) * rp.eta_three
    };

    let write_term = wire(p, Wire::Table1)
        + rp.gamma
        + wire(p, Wire::Table2) * rp.eta
        + wire(p, Wire::Table3) * rp.eta_two
        + wire(p, Wire::Table4) * rp.eta_three;

    let write_inverse = read_term * inverses;
    let read_inverse = write_term * inverses;

    evals[4] = (read_term * write_term * inverses - inverse_exists) * domain_sep;
    evals[5] = read_inverse * read_selector - write_inverse * read_counts;
    evals[6] = (read_tag * read_tag - read_tag) * domain_sep;
}

// 7..=10
fn accumulate_delta_range_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let q_delta_range = wire(p, Wire::QRange);
    let minus_one = Fr::zero() - Fr::one();
    let minus_two = Fr::zero() - Fr::from_u64(2);

    let deltas = [
        wire(p, Wire::Wr) - wire(p, Wire::Wl),
        wire(p, Wire::Wo) - wire(p, Wire::Wr),
        wire(p, Wire::W4) - wire(p, Wire::Wo),
        wire(p, Wire::WlShift) - wire(p, Wire::W4),
    ];

    for i in 0..4 {
        let mut acc = (deltas[i] + minus_one) * (deltas[i] + minus_one) + minus_one;
        acc = acc * ((deltas[i] + minus_two) * (deltas[i] + minus_two) + minus_one);
        evals[7 + i] = acc * q_delta_range * domain_sep;
    }
}

// 11..=12
fn accumulate_elliptic_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let x_1 = wire(p, Wire::Wr);
    let y_1 = wire(p, Wire::Wo);
    let x_2 = wire(p, Wire::WlShift);
    let y_2 = wire(p, Wire::W4Shift);
    let y_3 = wire(p, Wire::WoShift);
    let x_3 = wire(p, Wire::WrShift);

    let q_sign = wire(p, Wire::Ql);
    let q_elliptic = wire(p, Wire::QElliptic);
    let q_is_double = wire(p, Wire::Qm);

    let x_diff = x_2 - x_1;
    let y2_sqr = y_2 * y_2;
    let y1_sqr = y_1 * y_1;
    let y1y2 = y_1 * y_2 * q_sign;
    let x_add_identity = (x_3 + x_2 + x_1) * x_diff * x_diff - y2_sqr - y1_sqr + y1y2 + y1y2;

    let q_elliptic_by_scaling = q_elliptic * domain_sep;
    let q_elliptic_q_double_scaling = q_elliptic_by_scaling * q_is_double;
    let q_elliptic_not_double_scaling = q_elliptic_by_scaling - q_elliptic_q_double_scaling;

    let mut tmp_1 = x_add_identity * q_elliptic_not_double_scaling;

    let y1_plus_y3 = y_1 + y_3;
    let y_diff = y_2 * q_sign - y_1;
    let y_add_identity = y1_plus_y3 * x_diff + (x_3 - x_1) * y_diff;
    let mut tmp_2 = y_add_identity * q_elliptic_not_double_scaling;

    let x1_mul_3 = x_1 + x_1 + x_1;
    let x_pow_4_mul_3 = (y1_sqr - curve_b()) * x1_mul_3;
    let y1_sqr_mul_4 = y1_sqr + y1_sqr + y1_sqr + y1_sqr;
    let x1_pow_4_mul_9 = x_pow_4_mul_3 + x_pow_4_mul_3 + x_pow_4_mul_3;
    let x_double_identity = (x_3 + x_1 + x_1) * y1_sqr_mul_4 - x1_pow_4_mul_9;
    tmp_1 = tmp_1 + x_double_identity * q_elliptic_q_double_scaling;

    let x1_sqr_mul_3 = x1_mul_3 * x_1;
    let y_double_identity = x1_sqr_mul_3 * (x_1 - x_3) - (y_1 + y_1) * y1_plus_y3;
    tmp_2 = tmp_2 + y_double_identity * q_elliptic_q_double_scaling;

    evals[11] = tmp_1;
    evals[12] = tmp_2;
}

// 13..=18
fn accumulate_memory_relation(p: &[Fr], rp: &RelationParameters, evals: &mut [Fr], domain_sep: Fr) {
    let eta = rp.eta;
    let eta_two = rp.eta_two;
    let eta_three = rp.eta_three;

    let w_1 = wire(p, Wire::Wl);
    let w_2 = wire(p, Wire::Wr);
    let w_3 = wire(p, Wire::Wo);
    let w_4 = wire(p, Wire::W4);
    let w_1_shift = wire(p, Wire::WlShift);
    let w_2_shift = wire(p, Wire::WrShift);
    let w_3_shift = wire(p, Wire::WoShift);
    let w_4_shift = wire(p, Wire::W4Shift);

    let q_1 = wire(p, Wire::Ql);
    let q_2 = wire(p, Wire::Qr);
    let q_3 = wire(p, Wire::Qo);
    let q_4 = wire(p, Wire::Q4);
    let q_m = wire(p, Wire::Qm);
    let q_c = wire(p, Wire::Qc);
    let q_memory = wire(p, Wire::QAux);

    let mut memory_record_check = w_3 * eta_three + w_2 * eta_two + w_1 * eta + q_c;
    let partial_record_check = memory_record_check;
    memory_record_check = memory_record_check - w_4;

    let neg_index_delta = w_1 - w_1_shift;
    let index_delta_is_zero = neg_index_delta + Fr::one();
    let record_delta = w_4_shift - w_4;
    let index_is_monotonically_increasing = neg_index_delta * neg_index_delta + neg_index_delta;
    let adjacent_values_match_if_adjacent_indices_match = index_delta_is_zero * record_delta;

    let q_memory_by_scaling = q_memory * domain_sep;
    let q_one_by_two = q_1 * q_2;
    let q_one_by_two_by_memory_by_scaling = q_one_by_two * q_memory_by_scaling;

    evals[14] = adjacent_values_match_if_adjacent_indices_match * q_one_by_two_by_memory_by_scaling;
    evals[15] = index_is_monotonically_increasing * q_one_by_two_by_memory_by_scaling;

    let rom_consistency_check_identity = memory_record_check * q_one_by_two;

    let neg_access_type = partial_record_check - w_4;
    let access_check = neg_access_type * neg_access_type + neg_access_type;

    let mut neg_next_gate_access_type =
        w_3_shift * eta_three + w_2_shift * eta_two + w_1_shift * eta;
    neg_next_gate_access_type = neg_next_gate_access_type - w_4_shift;
    let value_delta = w_3_shift - w_3;

    let adjacent_values_match_if_adjacent_indices_match_and_next_access_is_a_read_operation =
        (index_delta_is_zero * value_delta) * (neg_next_gate_access_type + Fr::one());

    let next_gate_access_type_is_boolean =
        neg_next_gate_access_type * neg_next_gate_access_type + neg_next_gate_access_type;

    let q_3_by_memory_and_scaling = q_3 * q_memory_by_scaling;
    evals[16] = adjacent_values_match_if_adjacent_indices_match_and_next_access_is_a_read_operation
        * q_3_by_memory_and_scaling;
    evals[17] = index_is_monotonically_increasing * q_3_by_memory_and_scaling;
    evals[18] = next_gate_access_type_is_boolean * q_3_by_memory_and_scaling;

    let ram_consistency_check_identity = access_check * q_3_by_memory_and_scaling;

    let timestamp_delta = w_2_shift - w_2;
    let ram_timestamp_check_identity = index_delta_is_zero * timestamp_delta - w_3;

    let mut memory_identity = rom_consistency_check_identity;
    memory_identity = memory_identity + ram_timestamp_check_identity * (q_4 * q_1);
    memory_identity = memory_identity + memory_record_check * (q_m * q_1);
    memory_identity = memory_identity * q_memory_by_scaling;
    memory_identity = memory_identity + ram_consistency_check_identity;

    evals[13] = memory_identity;
}

// 19
fn accumulate_non_native_field_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let w_1 = wire(p, Wire::Wl);
    let w_2 = wire(p, Wire::Wr);
    let w_3 = wire(p, Wire::Wo);
    let w_4 = wire(p, Wire::W4);
    let w_1_shift = wire(p, Wire::WlShift);
    let w_2_shift = wire(p, Wire::WrShift);
    let w_3_shift = wire(p, Wire::WoShift);
    let w_4_shift = wire(p, Wire::W4Shift);

    let q_2 = wire(p, Wire::Qr);
    let q_3 = wire(p, Wire::Qo);
    let q_4 = wire(p, Wire::Q4);
    let q_m = wire(p, Wire::Qm);
    let q_nnf = wire(p, Wire::QNnf);

    let mut limb_subproduct = w_1 * w_2_shift + w_1_shift * w_2;
    let mut non_native_field_gate_2 = w_1 * w_4 + w_2 * w_3 - w_3_shift;
    non_native_field_gate_2 = non_native_field_gate_2 * limb_size();
    non_native_field_gate_2 = non_native_field_gate_2 - w_4_shift;
    non_native_field_gate_2 = non_native_field_gate_2 + limb_subproduct;
    non_native_field_gate_2 = non_native_field_gate_2 * q_4;

    limb_subproduct = limb_subproduct * limb_size();
    limb_subproduct = limb_subproduct + w_1_shift * w_2_shift;

    let mut non_native_field_gate_1 = limb_subproduct - (w_3 + w_4);
    non_native_field_gate_1 = non_native_field_gate_1 * q_3;

    let mut non_native_field_gate_3 = limb_subproduct + w_4;
    non_native_field_gate_3 = non_native_field_gate_3 - (w_3_shift + w_4_shift);
    non_native_field_gate_3 = non_native_field_gate_3 * q_m;

    let mut non_native_field_identity =
        non_native_field_gate_1 + non_native_field_gate_2 + non_native_field_gate_3;
    non_native_field_identity = non_native_field_identity * q_2;

    let mut limb_accumulator_1 = w_2_shift * sublimb_shift() + w_1_shift;
    limb_accumulator_1 = limb_accumulator_1 * sublimb_shift() + w_3;
    limb_accumulator_1 = limb_accumulator_1 * sublimb_shift() + w_2;
    limb_accumulator_1 = limb_accumulator_1 * sublimb_shift() + w_1;
    limb_accumulator_1 = (limb_accumulator_1 - w_4) * q_4;

    let mut limb_accumulator_2 = w_3_shift * sublimb_shift() + w_2_shift;
    limb_accumulator_2 = limb_accumulator_2 * sublimb_shift() + w_1_shift;
    limb_accumulator_2 = limb_accumulator_2 * sublimb_shift() + w_4;
    limb_accumulator_2 = limb_accumulator_2 * sublimb_shift() + w_3;
    limb_accumulator_2 = (limb_accumulator_2 - w_4_shift) * q_m;

    let limb_accumulator_identity = (limb_accumulator_1 + limb_accumulator_2) * q_3;

    let mut nnf_identity = non_native_field_identity + limb_accumulator_identity;
    nnf_identity = nnf_identity * q_nnf * domain_sep;

    evals[19] = nnf_identity;
}

// 20..=23
fn accumulate_poseidon_external_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let s1 = wire(p, Wire::Wl) + wire(p, Wire::Ql);
    let s2 = wire(p, Wire::Wr) + wire(p, Wire::Qr);
    let s3 = wire(p, Wire::Wo) + wire(p, Wire::Qo);
    let s4 = wire(p, Wire::W4) + wire(p, Wire::Q4);

    let u1 = s1.pow(5);
    let u2 = s2.pow(5);
    let u3 = s3.pow(5);
    let u4 = s4.pow(5);

    let t0 = u1 + u2;
    let t1 = u3 + u4;
    let t2 = u2 + u2 + t1;
    let t3 = u4 + u4 + t0;

    let v4 = t1 + t1 + t1 + t1 + t3;
    let v2 = t0 + t0 + t0 + t0 + t2;
    let v1 = t3 + v2;
    let v3 = t2 + v4;

    let q_poseidon = wire(p, Wire::QPoseidon2External) * domain_sep;
    evals[20] = (v1 - wire(p, Wire::WlShift)) * q_poseidon;
    evals[21] = (v2 - wire(p, Wire::WrShift)) * q_poseidon;
    evals[22] = (v3 - wire(p, Wire::WoShift)) * q_poseidon;
    evals[23] = (v4 - wire(p, Wire::W4Shift)) * q_poseidon;
}

// 24..=27
fn accumulate_poseidon_internal_relation(p: &[Fr], evals: &mut [Fr], domain_sep: Fr) {
    let u1 = (wire(p, Wire::Wl) + wire(p, Wire::Ql)).pow(5);
    let u2 = wire(p, Wire::Wr);
    let u3 = wire(p, Wire::Wo);
    let u4 = wire(p, Wire::W4);

    let u_sum = u1 + u2 + u3 + u4;
    let d = internal_matrix_diagonal();
    let w1 = u1 * d[0] + u_sum;
    let w2 = u2 * d[1] + u_sum;
    let w3 = u3 * d[2] + u_sum;
    let w4 = u4 * d[3] + u_sum;

    let q_poseidon = wire(p, Wire::QPoseidon2Internal) * domain_sep;
    evals[24] = (w1 - wire(p, Wire::WlShift)) * q_poseidon;
    evals[25] = (w2 - wire(p, Wire::WrShift)) * q_poseidon;
    evals[26] = (w3 - wire(p, Wire::WoShift)) * q_poseidon;
    evals[27] = (w4 - wire(p, Wire::W4Shift)) * q_poseidon;
}

#[inline(always)]
fn scale_and_batch_subrelations(evaluations: &[Fr], subrelation_challenges: &[Fr]) -> Fr {
    let mut accumulator = evaluations[0];
    for i in 1..NUMBER_OF_SUBRELATIONS {
        accumulator = accumulator + evaluations[i] * subrelation_challenges[i - 1];
    }
    accumulator
}

pub fn accumulate_relation_evaluations(
    purported_evaluations: &[Fr],
    rp: &RelationParameters,
    alphas: &[Fr],
    pow_partial_eval: Fr,
) -> Fr {
    let mut evaluations = [Fr::zero(); NUMBER_OF_SUBRELATIONS];

    accumulate_arithmetic_relation(purported_evaluations, &mut evaluations, pow_partial_eval);
    accumulate_permutation_relation(
        purported_evaluations,
        rp,
        &mut evaluations,
        pow_partial_eval,
    );
    accumulate_log_derivative_lookup_relation(
        purported_evaluations,
        rp,
        &mut evaluations,
        pow_partial_eval,
    );
    accumulate_delta_range_relation(purported_evaluations, &mut evaluations, pow_partial_eval);
    accumulate_elliptic_relation(purported_evaluations, &mut evaluations, pow_partial_eval);
    accumulate_memory_relation(
        purported_evaluations,
        rp,
        &mut evaluations,
        pow_partial_eval,
    );
    accumulate_non_native_field_relation(purported_evaluations, &mut evaluations, pow_partial_eval);
    accumulate_poseidon_external_relation(
        purported_evaluations,
        &mut evaluations,
        pow_partial_eval,
    );
    accumulate_poseidon_internal_relation(
        purported_evaluations,
        &mut evaluations,
        pow_partial_eval,
    );

    scale_and_batch_subrelations(&evaluations, alphas)
}
