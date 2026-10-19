# Aggregator Conflict Resolution Policy

When scores are replicated across multiple shards, the aggregator must resolve
conflicts between potentially different values from each shard. The
`ConflictPolicy` enum controls which strategy is used.

Configuration conflicts are handled separately from score conflicts. Use
[`aggregator-split-brain.md`](aggregator-split-brain.md) and
`detect_split_brain(wallet, asset_pair)` to detect shard configuration drift
before relying on a fan-out read.

## Policy Variants

### `HighestScore` (default)

Pick the shard whose score (or aggregate score) is numerically highest.

- `get_score`: selects the `RiskScore` with the largest `score`
- `get_aggregate_score`: selects the `AggregateRiskScore` with the largest
  `aggregate_score`

This is the default policy (used if no policy has been explicitly set).

### `MostRecent`

Pick the shard whose timestamp is newest.

- `get_score`: selects the `RiskScore` with the largest `timestamp`
- `get_aggregate_score`: selects the `AggregateRiskScore` with the largest
  `last_updated`

Useful when score freshness matters more than magnitude (e.g. a rapidly changing
market).

### `Median`

Combine shard scores with the (lower) median instead of picking a single winner.

- `get_score`: sorts the shard `score` values ascending and returns the element
  at index `(n - 1) / 2` (integer division).
- `get_aggregate_score`: same rule applied to `aggregate_score`.

With an even number of shards the lower of the two middle values is chosen, so
the result never exceeds the true median. This is the safer (higher risk) side
when the two middle values disagree.

### `TrimmedMean`

Discard a governed fraction of the extreme values on each side, then average the
remainder.

- The trim fraction is set with `set_trim_fraction(Env, u32) -> Result<(), ScoreError>`
  and read with `get_trim_fraction(Env) -> u32`. It is expressed in basis points
  (1 bp = 0.01%), must be in `0..=2500` (0%–25%), and defaults to `0` (plain mean).
- For `n` shards, `k = (n * trim_bps) / 10_000` values are dropped from each
  end. If `2 * k >= n` the trim is clamped to `k = (n - 1) / 2` so at least one
  value survives.
- The remaining values are summed and divided by their count using integer
division, which truncates toward zero. Truncation rounds the combined score
*down*, i.e. toward the safer (higher risk) side.

### `MadRejection`

Median-absolute-deviation outlier rejection followed by a weighted mean.

1. Compute the median `m` of the shard scores (lower median, as above).
2. Compute the absolute deviations `|x_i - m|` and take their lower median `mad`.
3. Reject any shard whose deviation exceeds `mad * mad_multiplier_bps / 10_000`.
   The multiplier is set with `set_mad_multiplier(Env, u32) -> Result<(), ScoreError>`
   and read with `get_mad_multiplier(Env) -> u32`; it must be in `1..=100_000`
   (0.01x–10x) and defaults to `30000` (3x).
4. Average the surviving shards with the same integer division as `TrimmedMean`.

If every shard is rejected (e.g. `mad == 0` and all deviations are non-zero),
the policy falls back to the plain median `m` rather than returning an error.

## Shard Lifecycle

Shards move through a governed lifecycle state machine. The state is tracked per
shard and controls whether the shard counts toward quorum for new queries and
whether it may be removed from the registry.

### States

- **`Active`** — the shard is a full participant. It counts toward quorum for
  new queries and answers reads normally.
- **`Draining`** — the shard is read-only. It is **excluded from quorum
  calculations for new queries**, so it no longer contributes to the quorum
  threshold or the shard set of queries started after the transition. It is
  **not** removed: its data is retained and it still answers direct reads
  (`get_score`, `get_aggregate_score` against that shard) so operators can
  migrate or archive its unique state.
- **`Decommissioned`** — the shard is removed from the registry and no longer
  answers reads.

### Transitions

| From           | To               | Authorisation                         |
| -------------- | ---------------- | ------------------------------------- |
| `Active`       | `Draining`       | Admin                                 |
| `Draining`     | `Decommissioned` | Admin, after the minimum drain period |
| `Draining`     | `Active`         | Admin (abort a drain)                 |
| `Active`       | `Decommissioned` | Emergency override only (see below)   |

A shard cannot move from `Active` straight to `Decommissioned` through the
normal path. The only way to skip the drain period is the explicit emergency
override, which requires a distinct, higher authorisation than ordinary admin
actions.

### Minimum Drain Duration

The minimum drain duration is **governed**, not fixed. It is set with
`set_min_drain_duration(Env, u64) -> Result<(), ScoreError>` and read with
`get_min_drain_duration(Env) -> u64`, expressed in seconds. It defaults to a
non-zero value so a drain can never be completed instantly by accident.

Boundary behavior:

- A shard may move from `Draining` to `Decommissioned` only once
  `now - drain_started_at >= min_drain_duration`.
- Exactly at the boundary (`elapsed == min_drain_duration`) the transition is
  **allowed**; one second earlier (`elapsed == min_drain_duration - 1`) it is
  rejected with `ScoreError::DrainPeriodNotElapsed`.
- Changing the minimum drain duration does not retroactively shorten or extend a
  drain already in progress beyond what the elapsed time already satisfies; the
  check is always evaluated against the current governed value at transition
  time.

### Quorum Exclusion

Once a shard is `Draining`, it is excluded from quorum for **new** queries. The
quorum threshold and the participating shard set are computed from `Active`
shards only. This exclusion is **not retroactive**: a query that already
committed to including the shard before the transition keeps its original shard
set and quorum threshold for the duration of that query.

### In-Flight Query Guarantee

A query's result is determined by the shard set and quorum captured when the
query started. If a shard transitions (`Active` → `Draining`, or any other
lifecycle change) while a query is executing, that query's result is unaffected:
it still counts the shard as it was at start time and returns the same value it
would have returned had no transition occurred. This guarantees that no query
mid-flight is silently dropped or altered when a shard is decommissioned.

### Emergency Override

The emergency override allows an `Active` shard to move directly to
`Decommissioned`, skipping the drain period. Because this bypasses the safety
guarantee above, it requires a **separate, higher authorisation** than ordinary
admin actions — it is not satisfied by the normal admin key alone. The override
must be explicitly authorised and is recorded as such; it is intended only for
compromised or unrecoverable shards, not routine decommissioning.

## Degenerate Cases
=======

Robust statistics degenerate with very few shards. The policies are defined as
follows so behavior is always deterministic:

- **One shard:** every policy returns that shard's value unchanged.
- **Two shards:** `Median` returns the lower value. `TrimmedMean` clamps the trim
  to `k = 0` and returns the truncated mean of both values. `MadRejection`
  rejects nothing unless the multiplier is `0`, and returns the truncated mean.
- **Three shards:** `Median` returns the middle value. `TrimmedMean` with a
  non-zero trim fraction drops the lowest and highest and returns the middle
  value. `MadRejection` rejects only values farther than the multiplier times the
  MAD from the median.

## Missing and Stale Shards

Shards that do not respond, or whose response is older than the staleness
threshold, are excluded before any policy is applied. If fewer than one shard
remains the call returns `ScoreError::NoShardsAvailable`. If exactly one shard
remains, the policy returns its value unchanged (see above). Staleness is
measured against the same threshold used by `MostRecent`.

## Determinism and Rounding

All policies are deterministic and integer-only:

- Sorting is stable and ties are broken by shard index, so equal scores never
  change the result across runs.
- Averages use integer division, which truncates toward zero. Truncation lowers
  the combined score, i.e. it favors the safer (higher risk) side when the exact
  value is ambiguous.
- Medians use the lower middle element for even counts, again favoring the safer
  side.

## Cost Analysis

Let `n` be the number of responding shards. Costs are given in relative units
(sort comparisons plus arithmetic), ignoring the fixed fan-out cost.

| Policy         | Time        | Extra storage        | Max shards |
| -------------- | ----------- | -------------------- | ---------- |
| `HighestScore` | O(n)        | none                 | unbounded  |
| `MostRecent`   | O(n)        | none                 | unbounded  |
| `Median`       | O(n log n)  | none                 | 64         |
| `TrimmedMean`  | O(n log n)  | trim fraction (u32)  | 64         |
| `MadRejection` | O(n log n)  | multiplier (u32)     | 32         |

The sort-based policies are capped to bound worst-case gas: `Median` and
`TrimmedMean` accept at most 64 shards, and `MadRejection` (which sorts twice)
accepts at most 32. Fan-out reads that exceed the cap for the active policy
return `ScoreError::TooManyShards`; callers should fall back to `HighestScore`
or reduce the shard set.

## Choosing a Policy

- Use `HighestScore` (default) when shards are trusted and you want the most
  conservative single reading.
- Use `MostRecent` when freshness dominates and stale values are the main risk.
- Use `Median` when a small number of shards may be faulty or manipulated and you
  want a bounded influence from any single shard.
- Use `TrimmedMean` when you can govern an expected outlier fraction and want a
  smooth combined value; keep the trim fraction at or below the fraction of
  shards you expect to be bad.
- Use `MadRejection` when outliers are rare but potentially large; keep the
  multiplier at 3x or higher so legitimate spread is not rejected.

Safe configurations:

- Keep the shard count at or below the cap for the active policy (64 for
  `Median`/`TrimmedMean`, 32 for `MadRejection`).
- Keep `trim_bps <= 2500` and `mad_multiplier_bps >= 30000`.
- Always configure a staleness threshold so stale shards are excluded before
  aggregation.

## API

### `set_conflict_resolution_policy(Env, ConflictPolicy) -> Result<(), ScoreError>`

Sets the active policy. Requires admin authorization. Returns
`ScoreError::NotInitialized` if the contract has not been initialized.

### `get_conflict_resolution_policy(Env) -> ConflictPolicy`

Returns the current policy, or `ConflictPolicy::HighestScore` if none has been
set.

### `set_trim_fraction(Env, u32) -> Result<(), ScoreError>`

Sets the trimmed-mean trim fraction in basis points. Requires admin
authorization. Returns `ScoreError::InvalidParameter` if the value exceeds
`2500`.

### `get_trim_fraction(Env) -> u32`

Returns the current trim fraction in basis points, or `0` if unset.

### `set_mad_multiplier(Env, u32) -> Result<(), ScoreError>`

Sets the MAD rejection multiplier in basis points. Requires admin
authorization. Returns `ScoreError::InvalidParameter` if the value is `0` or
exceeds `100_000`.

### `get_mad_multiplier(Env) -> u32`

Returns the current MAD multiplier in basis points, or `30000` if unset.

### `set_min_drain_duration(Env, u64) -> Result<(), ScoreError>`

Sets the governed minimum drain duration in seconds. Requires admin
authorization. Returns `ScoreError::InvalidParameter` if the value is `0`.

### `get_min_drain_duration(Env) -> u64`

Returns the current minimum drain duration in seconds.

### `begin_shard_drain(Env, shard) -> Result<(), ScoreError>`

Moves an `Active` shard to `Draining` and records the drain start time. Requires
admin authorization. Returns `ScoreError::InvalidShardState` if the shard is not
`Active`.

### `complete_shard_decommission(Env, shard) -> Result<(), ScoreError>`

Moves a `Draining` shard to `Decommissioned`. Requires admin authorization and
that the minimum drain duration has elapsed. Returns
`ScoreError::DrainPeriodNotElapsed` if the drain period has not elapsed, and
`ScoreError::InvalidShardState` if the shard is not `Draining`.

### `emergency_decommission_shard(Env, shard) -> Result<(), ScoreError>`

Moves an `Active` shard directly to `Decommissioned`, bypassing the drain
period. Requires the distinct, higher emergency authorisation. Returns
`ScoreError::Unauthorized` if that authorisation is not present.
=======

## Example

```rust
// Set to MostRecent
client.set_conflict_resolution_policy(&ConflictPolicy::MostRecent);

// get_score now returns the score from the shard with the newest timestamp
let score = client.get_score(&wallet, &pair);

// Switch to a robust policy
client.set_conflict_resolution_policy(&ConflictPolicy::TrimmedMean);
client.set_trim_fraction(&1000); // drop 10% from each end

client.set_conflict_resolution_policy(&ConflictPolicy::MadRejection);
client.set_mad_multiplier(&30000); // reject beyond 3x MAD

// Switch back to HighestScore
client.set_conflict_resolution_policy(&ConflictPolicy::HighestScore);

// Govern the minimum drain duration (e.g. 24 hours)
client.set_min_drain_duration(&86_400);

// Drain a shard: it stops counting toward quorum for new queries but still
// answers direct reads while operators migrate its state.
client.begin_shard_drain(&shard);

// After the drain period elapses, complete the decommission.
client.complete_shard_decommission(&shard);
```
