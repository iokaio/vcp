// SPDX-License-Identifier: Apache-2.0
use vcp_models::markov::{self, compare_orders, Chain, Error, MAX_OBSERVATIONS, MAX_STATES};

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-8 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}
fn retry() -> Chain {
    Chain::new(vec![vec![0.5, 0.5], vec![0.0, 1.0]], vec![false, true]).unwrap()
}

#[test]
fn counts_never_bridge_segments_and_enforce_work_bounds() {
    assert_eq!(
        markov::count(3, &[&[0, 1, 1], &[], &[2, 0], &[1]]).unwrap(),
        vec![vec![0, 1, 0], vec![0, 1, 0], vec![1, 0, 0]]
    );
    assert_eq!(markov::count(3, &[&[3]]), Err(Error::Shape));
    assert_eq!(markov::count(0, &[]), Err(Error::Limit));
    assert_eq!(markov::count(MAX_STATES + 1, &[]), Err(Error::Limit));
    assert_eq!(
        markov::count(2, &[&vec![0; MAX_OBSERVATIONS + 1]]),
        Err(Error::Limit)
    );
    assert_eq!(
        markov::count(2, &vec![&[] as &[usize]; MAX_OBSERVATIONS + 1]),
        Err(Error::Limit)
    );
}

#[test]
fn hand_calculated_resumable_state_and_two_outcomes() {
    // State 1 is blocked but may resume state 0; it is not an absorbing outcome.
    let chain = Chain::new(
        vec![
            vec![0.5, 0.25, 0.25, 0.0],
            vec![0.5, 0.0, 0.0, 0.5],
            vec![0.0, 0.0, 1.0, 0.0],
            vec![0.0, 0.0, 0.0, 1.0],
        ],
        vec![false, false, true, true],
    )
    .unwrap();
    let result = chain
        .analyze(&[Some(3.0), Some(6.0), Some(0.0), Some(0.0)])
        .unwrap();
    assert_eq!(result.transient_states, vec![0, 1]);
    assert_eq!(result.absorbing_states, vec![2, 3]);
    for (actual, expected) in
        result
            .expected_visits
            .iter()
            .flatten()
            .zip([8.0 / 3.0, 2.0 / 3.0, 4.0 / 3.0, 4.0 / 3.0])
    {
        close(*actual, expected);
    }
    for (actual, expected) in result.outcome_probabilities.iter().flatten().zip([
        2.0 / 3.0,
        1.0 / 3.0,
        1.0 / 3.0,
        2.0 / 3.0,
    ]) {
        close(*actual, expected);
    }
    close(result.expected_rewards[0], 12.0);
    close(result.expected_rewards[1], 12.0);
}

#[test]
fn pivoting_state_order_and_terminal_only_models() {
    // First elimination column requires swapping rows; N = [[50, 5], [40, 5]].
    let chain = Chain::new(
        vec![
            vec![0.9, 0.0, 0.1],
            vec![0.0, 1.0, 0.0],
            vec![0.8, 0.2, 0.0],
        ],
        vec![false, true, false],
    )
    .unwrap();
    let result = chain.analyze(&[Some(1.0), Some(0.0), Some(1.0)]).unwrap();
    assert_eq!(result.transient_states, vec![0, 2]);
    assert_eq!(result.absorbing_states, vec![1]);
    for (actual, expected) in result
        .expected_visits
        .iter()
        .flatten()
        .zip([50.0, 5.0, 40.0, 5.0])
    {
        close(*actual, expected);
    }
    close(result.expected_rewards[0], 55.0);
    close(result.expected_rewards[1], 45.0);
    let terminal = Chain::new(vec![vec![1.0]], vec![true])
        .unwrap()
        .analyze(&[Some(0.0)])
        .unwrap();
    assert!(terminal.expected_visits.is_empty() && terminal.expected_rewards.is_empty());
    assert_eq!(terminal.absorbing_states, vec![0]);
}

#[test]
fn smoothing_respects_observed_support_and_raw_sample_gates() {
    let counts = vec![vec![1, 3, 0], vec![0, 0, 2], vec![0, 0, 0]];
    let legal = vec![vec![true; 3]; 3];
    let chain = Chain::fit(&counts, &legal, vec![false, false, true], 1.0, 2).unwrap();
    close(chain.probabilities()[0][0], 1.0 / 3.0);
    close(chain.probabilities()[0][1], 2.0 / 3.0);
    assert_eq!(chain.probabilities()[0][2], 0.0); // legal but not observed
    assert_eq!(
        Chain::fit(&counts, &legal, vec![false, false, true], 1e9, 3).unwrap_err(),
        Error::Sparse
    );
    let trapped = vec![vec![2, 0], vec![0, 0]];
    assert_eq!(
        Chain::fit(
            &trapped,
            &vec![vec![true; 2]; 2],
            vec![false, true],
            100.0,
            1
        )
        .unwrap_err(),
        Error::Nonabsorbing
    );
    let mut forbidden = legal.clone();
    forbidden[0][1] = false;
    assert_eq!(
        Chain::fit(&counts, &forbidden, vec![false, false, true], 0.0, 1).unwrap_err(),
        Error::Forbidden
    );
}

#[test]
fn invalid_sparse_overflow_and_nonfinite_fit_inputs_abstain() {
    let counts = vec![vec![0, 2], vec![0, 0]];
    let legal = vec![vec![true; 2]; 2];
    for prior in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(
            Chain::fit(&counts, &legal, vec![false, true], prior, 1).unwrap_err(),
            Error::Invalid
        );
    }
    assert_eq!(
        Chain::fit(&counts, &legal, vec![false, true], 0.0, 0).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        Chain::fit(&[vec![0, 0], vec![0, 0]], &legal, vec![false, true], 1.0, 1).unwrap_err(),
        Error::Sparse
    );
    assert_eq!(
        Chain::fit(
            &[vec![u64::MAX, 1], vec![0, 0]],
            &legal,
            vec![false, true],
            0.0,
            1
        )
        .unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        Chain::fit(
            &[vec![1, 1], vec![0, 0]],
            &legal,
            vec![false, true],
            f64::MAX,
            1
        )
        .unwrap_err(),
        Error::Numerical
    );
    assert_eq!(
        Chain::fit(&[vec![0, 1], vec![1, 0]], &legal, vec![false, true], 0.0, 1).unwrap_err(),
        Error::Forbidden
    );
    assert_eq!(
        Chain::fit(&counts, &[vec![true]], vec![false, true], 0.0, 1).unwrap_err(),
        Error::Shape
    );
}

#[test]
fn malformed_nonabsorbing_and_ill_conditioned_chains_abstain() {
    for row in [
        vec![f64::NAN, 0.5],
        vec![f64::INFINITY, 0.0],
        vec![-0.5, 1.5],
        vec![0.4, 0.5],
    ] {
        assert_eq!(
            Chain::new(vec![row, vec![0.0, 1.0]], vec![false, true]).unwrap_err(),
            Error::Invalid
        );
    }
    assert_eq!(
        Chain::new(vec![vec![1.0]], vec![]).unwrap_err(),
        Error::Shape
    );
    assert_eq!(
        Chain::new(vec![vec![1.0]], vec![false]).unwrap_err(),
        Error::Nonabsorbing
    );
    assert_eq!(
        Chain::new(vec![vec![0.0, 1.0], vec![0.0, 1.0]], vec![true, true]).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        Chain::new(vec![vec![1.0, 0.0], vec![0.0, 1.0]], vec![false, true]).unwrap_err(),
        Error::Nonabsorbing
    );
    let near_singular = Chain::new(
        vec![vec![1.0 - 1e-14, 1e-14], vec![0.0, 1.0]],
        vec![false, true],
    )
    .unwrap();
    assert_eq!(
        near_singular.analyze(&[Some(1.0), Some(0.0)]).unwrap_err(),
        Error::Numerical
    );
}

#[test]
fn log_likelihood_handles_long_sequences_without_underflow() {
    let chain = retry();
    let sequence = vec![0; 10_001];
    close(
        chain.log_likelihood(&sequence).unwrap().unwrap(),
        10_000.0 * 0.5f64.ln(),
    );
    assert_eq!(chain.log_likelihood(&[1, 0]).unwrap(), None);
    assert_eq!(chain.log_likelihood(&[1, 0, 2]), Err(Error::Shape));
    assert_eq!(chain.log_likelihood(&[]).unwrap(), Some(0.0));
    assert_eq!(chain.log_likelihood(&[0]).unwrap(), Some(0.0));
    assert_eq!(
        chain.log_likelihood(&vec![0; MAX_OBSERVATIONS + 1]),
        Err(Error::Limit)
    );
}

#[test]
fn unknown_invalid_or_overflowing_rewards_are_not_zero_cost() {
    let chain = retry();
    assert_eq!(
        chain.analyze(&[None, Some(0.0)]).unwrap_err(),
        Error::UnknownReward
    );
    assert_eq!(
        chain.analyze(&[Some(1.0), None]).unwrap_err(),
        Error::UnknownReward
    );
    for reward in [f64::NAN, f64::INFINITY, -1.0] {
        assert_eq!(
            chain.analyze(&[Some(reward), Some(0.0)]).unwrap_err(),
            Error::Invalid
        );
    }
    assert_eq!(
        chain.analyze(&[Some(1.0), Some(1.0)]).unwrap_err(),
        Error::Invalid
    );
    assert_eq!(
        chain.analyze(&[Some(f64::MAX), Some(0.0)]).unwrap_err(),
        Error::Numerical
    );
    assert_eq!(chain.analyze(&[]).unwrap_err(), Error::Shape);
    close(
        chain
            .analyze(&[Some(3.0), Some(0.0)])
            .unwrap()
            .expected_rewards[0],
        6.0,
    );
}

#[test]
fn heldout_order_comparison_penalizes_complexity_and_checks_two_step_frequency() {
    let patterned = [0usize, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1];
    let comparison = compare_orders(2, &[&patterned, &patterned], &[&patterned], 0.0, 1).unwrap();
    assert_eq!(comparison.heldout_predictions, 10);
    assert_eq!(comparison.selected_order, 2);
    assert!(comparison.second_log_likelihood > comparison.first_log_likelihood);
    assert!(comparison.second_penalized_score > comparison.first_penalized_score);
    assert!(comparison.first_order_two_step_max_abs_error > 0.0);

    let deterministic = [0usize, 1, 1, 1, 1, 1];
    let simpler = compare_orders(
        2,
        &[&deterministic, &deterministic],
        &[&deterministic],
        0.5,
        1,
    )
    .unwrap();
    assert_eq!(simpler.selected_order, 1);
    assert_eq!(simpler.first_parameters, 0);
    assert_eq!(simpler.second_parameters, 0);
}

#[test]
fn heldout_order_comparison_abstains_on_missing_support_and_bounds() {
    assert_eq!(
        compare_orders(2, &[&[0, 0, 1]], &[&[1, 1, 0]], 0.0, 1),
        Err(Error::Sparse)
    );
    assert_eq!(
        compare_orders(2, &[&[0, 1, 0]], &[&[0, 1]], 0.0, 1),
        Err(Error::Sparse)
    );
    assert_eq!(
        compare_orders(2, &[&[0, 1, 0]], &[&[0, 1, 0]], -1.0, 1),
        Err(Error::Invalid)
    );
    assert_eq!(
        compare_orders(2, &[&[0, 2, 0]], &[&[0, 1, 0]], 0.0, 1),
        Err(Error::Shape)
    );
    assert_eq!(
        compare_orders(2, &[&[0, 0, 1, 0, 0, 1]], &[&[0, 0, 1]], f64::MAX, 1),
        Err(Error::Numerical)
    );
    let training = vec![0; MAX_OBSERVATIONS / 2 + 1];
    let heldout = vec![0; MAX_OBSERVATIONS / 2];
    assert_eq!(
        compare_orders(2, &[&training], &[&heldout], 0.0, 1),
        Err(Error::Limit)
    );
}

#[test]
fn seeded_models_match_independent_finite_horizon_mass_propagation() {
    // Synthetic M9 starting fixture. Fixed integer generator, no fitted/private
    // traces. Each row has >= 1/2 terminal mass, bounding the 100-step remainder.
    let mut seed = 0x5eed_u64;
    for case in 0..32 {
        let mut probabilities = vec![vec![0.0; 5]; 5];
        for row in probabilities.iter_mut().take(3) {
            for probability in row.iter_mut().take(3) {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                *probability = ((seed >> 32) % 100) as f64 / 600.0;
            }
            let rest = 1.0 - row[..3].iter().sum::<f64>();
            row[3] = rest * 0.25;
            row[4] = rest * 0.75;
        }
        probabilities[3][3] = 1.0;
        probabilities[4][4] = 1.0;
        let analysis = Chain::new(probabilities.clone(), vec![false, false, false, true, true])
            .unwrap()
            .analyze(&[Some(1.0), Some(2.0), Some(3.0), Some(0.0), Some(0.0)])
            .unwrap();
        for start in 0..3 {
            let mut mass = [0.0; 5];
            mass[start] = 1.0;
            let mut visits = [0.0; 3];
            for _ in 0..100 {
                for state in 0..3 {
                    visits[state] += mass[state];
                }
                let mut next = [0.0; 5];
                for from in 0..5 {
                    for to in 0..5 {
                        next[to] += mass[from] * probabilities[from][to];
                    }
                }
                mass = next;
            }
            for state in 0..3 {
                close(analysis.expected_visits[start][state], visits[state]);
            }
            close(analysis.outcome_probabilities[start][0], mass[3]);
            close(analysis.outcome_probabilities[start][1], mass[4]);
            close(
                analysis.expected_rewards[start],
                visits[0] + 2.0 * visits[1] + 3.0 * visits[2],
            );
            assert!(mass[..3].iter().sum::<f64>() < 1e-25, "case {case}");
        }
    }
}
