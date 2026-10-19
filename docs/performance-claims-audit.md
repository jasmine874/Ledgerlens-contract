# Performance and Resource-Cost Claims Audit

This document records a dedicated audit of every specific, falsifiable performance or resource-cost claim made in the project's documentation **outside** the generated, CI-checked cost handbook. The generated handbook is verified automatically against current measurements; prose claims in architecture descriptions, design rationales, and threat models are not, and can silently drift as the implementation evolves.

This audit is a point-in-time snapshot. It is re-verified periodically via the release checklist (see `docs/release-process.md`, "Re-verify documented performance claims"). When re-verifying, update the "Verified" column with the date and measurement, and correct or flag any claim that has drifted.

## Scope

- **In scope:** specific, falsifiable performance or resource-cost claims in prose — a stated number, an order-of-magnitude comparison, or a claimed complexity bound — made as part of a design rationale.
- **Out of scope:** the generated cost handbook (CI-checked), purely qualitative statements ("fast", "cheap") with no falsifiable content, and claims about correctness rather than cost.

## Method

For each claim we recorded:

1. The document and the exact claim.
2. A fresh, current measurement or analysis used to verify it (benchmark, complexity analysis, or direct instrumentation).
3. A verdict: **Accurate**, **Stale** (was true, no longer), or **Wrong** (never true / mis-stated).
4. The correction applied, being explicit about what changed since the claim was written.
5. A **maintainer-review flag** where the corrected figure undermines the original justification for a documented design decision — these are not silently fixed.

## Findings

### 1. `docs/score-math.md` — scoring complexity bound

- **Claim:** Scoring a batch of `n` entries is `O(n)` in the number of entries, with constant per-entry work independent of batch size.
- **Verification:** Complexity analysis of the scoring loop plus a benchmark at `n = 10^2, 10^3, 10^4, 10^5`. Per-entry time was flat across batch sizes within measurement noise; total time scaled linearly.
- **Verdict:** **Accurate.**
- **Correction:** None required. Added a note that the bound assumes the per-entry path does not perform any batch-wide sort or scan, which is the current implementation.
- **Maintainer review:** Not flagged.

### 2. `docs/score-math.md` — per-entry arithmetic cost

- **Claim:** Each entry's score is computed with a fixed, small number of arithmetic operations (constant work per entry).
- **Verification:** Inspected the scoring routine and counted operations on the hot path; confirmed no data-dependent loops or allocations proportional to entry magnitude.
- **Verdict:** **Accurate.**
- **Correction:** None required.
- **Maintainer review:** Not flagged.

### 3. `docs/liquidity-threat-model.md` — adversary cost to manipulate liquidity

- **Claim:** An adversary must control a stated fraction of liquidity (order-of-magnitude figure) to move the reported liquidity metric by a meaningful amount, and the cost of doing so scales with the square of the target displacement.
- **Verification:** Re-derived the displacement-vs-controlled-fraction relationship from the current liquidity aggregation formula and cross-checked against a simulation at several controlled fractions.
- **Verdict:** **Stale.** The aggregation formula changed since the claim was written (weighting was adjusted), so the constant in the cost relationship no longer matches the documented figure. The *shape* of the relationship (super-linear cost in displacement) still holds; the specific constant does not.
- **Correction:** Updated the stated fraction/constant to the current measured value and added an explicit note: *"Corrected during the performance-claims audit — the aggregation weighting changed since this figure was first written; the super-linear cost relationship is unchanged but the constant has been re-measured."*
- **Maintainer review:** **Flagged.** The threat model's conclusion (that manipulation is economically unattractive) depends on this constant. The corrected constant is *less* favorable than the original claim, so the margin of safety is smaller than documented. **Maintainers must confirm the design decision (relying on economic deterrence rather than an additional guard) still holds given the corrected figure.** This is recorded here rather than silently fixed.

### 4. `docs/liquidity-threat-model.md` — resource cost of monitoring

- **Claim:** Continuous monitoring of liquidity state requires resources proportional to the number of tracked pools, i.e. `O(pools)` memory and per-tick work.
- **Verification:** Instrumented the monitoring path; memory and per-tick work both scaled linearly with tracked pools.
- **Verdict:** **Accurate.**
- **Correction:** None required.
- **Maintainer review:** Not flagged.

### 5. `docs/architecture.md` — design rationale cost comparison

- **Claim:** The chosen design avoids a per-request full recomputation, making steady-state cost an order of magnitude lower than the naive alternative.
- **Verification:** Benchmarked steady-state path against a naive full-recompute baseline at representative load.
- **Verdict:** **Stale.** The steady-state path gained incremental bookkeeping since the claim was written; the gap is now smaller than an order of magnitude (measured ratio recorded in the benchmark output). The design still avoids full recomputation, so the *qualitative* rationale holds, but the "order of magnitude" figure overstates the current advantage.
- **Correction:** Replaced the "order of magnitude" phrasing with the current measured ratio and added a note: *"Corrected during the performance-claims audit — incremental bookkeeping added since this rationale was written has narrowed the gap; the design still avoids full recomputation."*
- **Maintainer review:** **Flagged (low severity).** The design decision (avoid full recomputation) still holds, but the margin is smaller than the rationale implies. Maintainers should confirm the incremental bookkeeping is still worth its cost versus the narrowed advantage.

## Summary

| # | Document | Claim | Verdict | Maintainer review |
|---|----------|-------|---------|-------------------|
| 1 | `docs/score-math.md` | Scoring is `O(n)` | Accurate | No |
| 2 | `docs/score-math.md` | Constant per-entry work | Accurate | No |
| 3 | `docs/liquidity-threat-model.md` | Adversary cost constant | Stale | **Yes — design justification affected** |
| 4 | `docs/liquidity-threat-model.md` | Monitoring is `O(pools)` | Accurate | No |
| 5 | `docs/architecture.md` | Order-of-magnitude advantage | Stale | Yes (low severity) |

## Open items for maintainers

- **Finding 3** requires an explicit decision: does the liquidity threat model's reliance on economic deterrence still hold with the corrected (less favorable) constant, or is an additional guard warranted? Do not close this item without a recorded decision.
- **Finding 5** requires confirmation that the incremental bookkeeping's cost is justified by the narrowed (but still real) advantage over full recomputation.

## Re-verification

This audit is not a one-time pass. The release process includes a checklist item to re-verify the highest-value claims (currently findings 3 and 5, plus any new prose claims added since the last audit) against fresh measurements. See `docs/release-process.md`.
</｜｜DSML｜｜ parameter>
</｜｜DSML｜｜ invoke>
</｜｜DSML｜｜ calls>