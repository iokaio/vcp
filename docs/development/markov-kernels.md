# Bounded Markov arithmetic — P6-02 / M1

`vcp-models::markov` implements pure local arithmetic under M1 step 5. It does
not read retained history, publish a fit or supply a qualified routing estimate.
The [task-transition inspector](../adr/029-retained-transition-evidence.md)
remains separate: its coarse counts do not supply the missing action, cohort,
counter or reward evidence needed by a consumer.

## Inputs and bounds

The alphabet has 1–32 states. Counting accepts separate contiguous segments and
never joins them; callers must split at task/attempt boundaries, missing revisions
and unavailable observations. Total input states and segment count each have a
100,000 bound. Sequence likelihood has the same observation bound.

`Chain::new` validates a square finite nonnegative stochastic matrix and an
explicit absorbing-state mask. Rows sum to one within `1e-12`; terminal rows must
be exact self-loops. Every transient state must have a positive-probability path
to a declared terminal. A blocked or paused state is not terminal merely because
its history ends. A closed transient class is rejected, including when another
class can complete.

`Chain::fit` accepts integer counts, a legality mask, a finite nonnegative prior
and an explicit positive minimum raw row sample count. Forbidden observations,
count overflow and sparse transient rows fail. Priors apply only to **observed
legal edges**, leaving unobserved edges at zero even when legal. Smoothing cannot
manufacture a path to success or satisfy a sample gate. Terminal self-loops are
structural and need no observed sample. Integer counts convert to floating point
only for normalization; count identity and monetary accounting remain external.

## Solving and rewards

`Chain::analyze` forms `I - Q` in original state-index order and computes its
inverse with partial-pivot Gauss-Jordan elimination. There are at most 32 pivots;
work is bounded by a cubic function of alphabet size. A pivot at or below
`1e-12`, non-finite or negative visit result, failed scaled `A*N = I` residual
(`1e-9` tolerance), or invalid terminal-probability sum causes abstention through
a typed error. No iterative convergence loop or external numerical package is
used. Near-singular chains can be conservatively rejected even if mathematically
absorbing. Cross-platform agreement uses tolerances, not bit-identical floats.

The result identifies transient and absorbing state indexes. `expected_visits`
includes the starting transient visit; row sums give expected transient steps.
`outcome_probabilities` has one column per terminal. Already-terminal starts are
trivial and omitted from result rows; a fully terminal input has no transient rows.

Rewards are nonnegative expected amounts per transient visit in caller-declared
units, with zero terminal reward. `None`, non-finite amounts and overflow fail;
unknown liability cannot be treated as zero. The caller must first settle
attribution, currency, parent/child rollups and late/duplicate receipts. These
results are neither integer money nor immediate reservation bounds, and are not
a probability of completing within a budget. Terminal charges need attribution
to the preceding transition/visit before use; the solver does not invent it.

`log_likelihood` sums log transition probabilities conditional on the first state.
An impossible sequence returns `None`; empty/single-state sequences return zero.
It fits no initial-state distribution and does not multiply tiny probabilities
until they underflow.

## Remaining integration

No serialization or persisted artifact format is added. Fit provenance, source
retention, cohort/sample qualification, uncertainty and consumed-value replay
remain M1 work; consumer activation remains M4. These functions cannot reach a
provider, alter policy or grant authority. See the
[verification record](../evaluations/p6-markov-kernels.md).
