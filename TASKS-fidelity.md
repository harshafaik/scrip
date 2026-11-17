# scrip — behavioral fidelity work list

Handoff brief. Repo: `~/Projects/scrip` (Rust, 4.3k LOC, single crate).
Goal: turn scrip from a statistically plausible generator into a *behaviorally* faithful
one, without breaking reproducibility or the domain-agnostic seam.

Read this whole file before writing code. Tasks are ordered; do not reorder without
saying why. Each task has an acceptance criterion that is a test, not a judgement call.

---

## Status (2026-11-17)

Done: T0 (file-based golden, `dataset_version`, metrics report), the behavioral
minimum of T8 (`scrip audit`), T1 (session-based arrivals), and P0-1 through
P0-7 (file metadata, partitioned output, as-of/window invariants, explicit base
date, valid IPs, dataset manifest, Date/Datetime dtypes). `cargo test --locked`
is green.

The golden fingerprint was refreshed after the P0 behavior changes, and
`transactions.timestamp` is written as `Datetime(us, UTC)` to match schema v2.
Remaining roadmap: T2–T7 and the full statistical/leakage audit layer.

Downstream note: `riskfabric-pipelines/fixtures/build_fixtures.py` partitions
on the schema-v2 `Datetime` column (`dt.date()`), so the v2 output is consumed
without string slicing. The ingester's cast path already accepts Datetime and
Date inputs.

## Hard constraints (violating these fails the task, not just the review)

1. **Determinism.** Same `(config, params)` must produce byte-identical output. No wall
   clock reads (`Utc::now()` is allowed only for `base_end_date`'s default in the CLI
   layer), no `HashMap` iteration order leaking into output, no global mutable state.
   Per-card `slice::par_iter` is fine because each card owns its RNG stream.
2. **The seam.** `src/core/**` must not reference `crate::config`, `crate::models`,
   `crate::generators`, `crate::fixtures`, `crate::reference`. Enforced by
   `tests/architecture.rs`. Domain-agnostic primitives (distributions, IET sampling,
   graph assignment) go in `core`; finance semantics stay in `generators`.
3. **Config-first.** Every new knob is a YAML field with a default. No magic numbers in
   code — the existing 80/15/3/2 spatial split and the `0.4..1.6` amount band are exactly
   the smell to avoid repeating.
4. **No new dependencies without asking.** `rand_distr` (for lognormal) is the only one
   I expect you to need. Propose it before adding.
5. **One behavior change per commit**, each with a before/after metric table in the
   commit message. Behavior changes invalidate `tests/golden.rs`; see T0.

---

## T0 — Make behavior change reviewable (do this first, it unblocks everything)

The golden test hardcodes `const GOLDEN: u64`. Right now any behavioral change is a
mystery-number swap with no evidence.

- Add a regen path: when `SCRIP_REGEN_GOLDEN=1`, the test prints the new fingerprint and
  passes; a helper (`cargo run --bin fingerprint` or a `#[ignore]` test) writes it.
- Add `dataset_version` to the emitted output (extend `injector_version` handling) so a
  consumer can tell which behavior produced a file.
- Add `tests/metrics_report.rs` — an ignored-by-default test that prints a table of the
  fidelity metrics defined in T8 for the standard 25-customer/30-day fixture.

**Acceptance:** `cargo test` green with no behavior change; `SCRIP_REGEN_GOLDEN=1 cargo
test golden` prints the fingerprint; `dataset_version` appears in at least the fraud
metadata output.

---

## T1 — Time as a process, not a sorted marginal (highest value)

Current: each transaction's timestamp is drawn independently from the hour × weekday
marginal, then the vector is sorted. Consequence: no inter-event-time structure, so no
velocity feature is learnable from legitimate traffic and fraud's injected burst is the
only burst in the dataset.

Replace with a per-card activity process:

- Draw **sessions**, not transactions. Session count per card from a Poisson/binomial
  scaled by the card's activity level (derive from `monthly_spend` and the configured
  txn count so total volume stays comparable to today's).
- Session start timestamp: sample day (weekday weights) then hour (channel-hourly
  weights). Weights now apply to session starts, not to individual transactions.
- Session duration: lognormal, clipped. Transactions per session: mostly 1, heavy tail
  (e.g. 1 with p≈0.7, 2–3 with p≈0.25, 4+ rare).
- Within-session IET: short (seconds to minutes), drawn from a configurable distribution.
- Inter-session gap: hours to days, lognormal; must allow same-day multiple sessions.

**Acceptance — revised.** The original criteria (IET autocorrelation > 0.05, and 2–8% of
cards with 3-in-60) were wrong instruments and are withdrawn. Measured on a probe:

    fixture             model   IET ac1   Fano   burst-mem%   cards w/burst%
    25c x 30d x 300tx   iid      -0.007   1.21       29.3%            100%
    25c x 30d x 300tx   sess     -0.038   1.77       48.6%            100%
    25c x 365d x 75tx   iid      -0.020   1.00        0.0%              0%
    25c x 365d x 75tx   sess     -0.065   1.56       11.5%             96%

A session model is *bimodal*, so its IET autocorrelation is legitimately ≈0 or negative —
positive lag-1 requires a self-exciting (Hawkes) process, which is out of scope. And
"share of cards with ≥3-in-60" saturates at ~100% under any real session model, so it
cannot discriminate.

Assert instead, on the standard fixture (25c / 30d):

- **Fano factor of hourly counts > 1.3** (iid gives 1.0–1.2; session model 1.6–1.8)
- **|lag-1 IET autocorrelation| < 0.10** — inverted deliberately. Together with the Fano
  bound this pins the model class: a sorted-iid model fails Fano, a self-exciting model
  fails autocorrelation, a renewal-with-regimes process passes both.
- **IET bimodality:** within-session p50 in minutes (10–600s), inter-session p50 in hours
  (> 2h), p90/p10 IET ratio > 20.

Assert bursts on a **sparse** fixture (25c / 365d / 75 txns per card), never the dense one:

- **burst-membership fraction** — share of *transactions* that sit inside some 60-minute
  window containing ≥3 transactions — between 5% and 25% (iid gives 0.0%, session 11.5%).
- Do not use a per-card indicator for this. It saturates and tells you nothing.

Also:

- Total transactions per card still matches `transactions_per_customer` sampling, so the
  CLI contract is unchanged.
- The dense 25/30 standard fixture is documented in the test as intentionally dense and
  excluded from the burst-band assertion.
- Bands above are derived from a probe with placeholder session parameters
  (70/25/4/1 sizes, 300s within-session mean, 18h inter-session mean). Once the real
  parameters are chosen, regenerate the predicted band from a simulation and record how
  it was obtained in the test comment — do not hardcode a number whose basis is unknown.
- Hide the arrival model behind a trait in `core` (e.g. `ArrivalProcess`) so a
  self-exciting variant can be swapped in later without touching `generators/`. Do not
  implement the Hawkes variant now.
- Golden updated via T0, fingerprint change documented.

---

## T2 — Enforce entity lifecycle

`Card` carries `issue_date`, `activation_date`, `status`; `Customer` carries
`registration_date`. `generators/transaction.rs` reads none of them. Consequences: a
Blocked/Expired card transacts normally, and a card issued inside the window still has a
full window of history.

- Add `status_changed_at` to `Card` (blocked/closed timestamps). No transaction may
  precede `activation_date` or follow `closed_at`; Blocked cards stop.
- Add dormancy: a fraction of cards go dormant for a stretch and reactivate.
- Cold start: cards issued inside the window get a truncated history (0 prior rows).
  This is the segment where every rolling feature is null — it must be present, not
  accidentally absent.

**Acceptance:** `tests/lifecycle.rs` asserts no transaction precedes activation or
follows close, that at least one card in the fixture has zero prior history at first
transaction, and that dormant cards have a gap ≥ the configured minimum.

---

## T3 — Reporting funnel instead of memoryless label noise

Current: `core/labels.rs` flips the label with independent per-transaction
`fn_rate`/`fp_rate`. That models label noise, not label *discovery*.

Restructure as a three-stage process:

1. fraud occurs (existing `target_share` path)
2. detected by a simulated rules layer at rate `d(segment, channel)` — configurable per
   fraud type, because card-present and UPI have different detection profiles
3. confirmed after a delay sampled from a distribution (replace the constant
   `chargeback_days: 30`)

Emit `label_available_at` per transaction. Keep the `label_noise` string for backwards
compatibility, but derive it from the funnel rather than from a Bernoulli.

**Acceptance:**
- Observed fraud rate < target rate, and the ratio is configurable.
- `label_available_at - timestamp` is a distribution, not a constant; test asserts ≥3
  distinct delays appear in the fixture.
- A test demonstrating cutoff behaviour: filtering rows to `label_available_at <= T`
  yields strictly fewer positives than the unflagged set.
- `core/labels.rs` stays in core only if it carries no config types; pass rates in as
  plain floats, as it does today.

---

## T4 — Shared infrastructure so rings exist

Current: `DeviceProfile` is a UA string from a pool plus an ISP subnet. Sharing happens
by config coincidence, so there is no ring structure and no graph signal.

- Introduce `device_id` (stable per customer) and a `SharedInfra` assignment at campaign
  init: a small number of device nodes and IPs are shared across N accounts with
  configurable fan-out (heavy-tailed, e.g. a few nodes with fan-out 5–20).
- Assign ring membership to a fraction of campaigns; ring members share a device, and
  optionally a beneficiary account.
- Emit `device_id` on the transaction and `ring_id` in fraud metadata.

**Acceptance:** `tests/graph.rs` computes the entity-projection graph over `device_id`
and asserts: max fan-out ≥ configured, global clustering coefficient > 0, and ring
members are recoverable from metadata. Also assert the *legitimate* population still has
mostly unique devices (don't turn every customer into a ring).

---

## T5 — Give amounts a real distribution

Current: `avg_txn_amount * U(0.4, 1.6)`, clamped. Bounded and symmetric, so there is no
long right tail and no legitimate "one big purchase" confounder.

- Replace with a lognormal (or two-component mixture) per customer, scaled so the annual
  sum lands within tolerance of `monthly_spend * 12`.
- Keep a small probability of a legitimate one-off large purchase (travel, electronics)
  drawn from the upper tail — this is a hard negative, not noise.

**Acceptance:** test asserts within-card skewness > 0 and p99/p50 ratio in a configured
band; and that per-card annual sum is within ±15% of `monthly_spend * 12`.

---

## T6 — Difficulty ladder + cost model

- `--difficulty easy|normal|hard` presets that scale: amount mimicry strength, share of
  hard-negative legitimate traffic, signal strength of each fraud mechanism.
- Per-transaction `expected_loss` = amount × P(not recovered) so decision metrics
  (expected cost, alerts/day at fixed precision) are computable from the dataset.

**Acceptance:** a documented table in `documentation/` of baseline metrics per
difficulty (AUC of a simple model, precision@k, expected cost). On `hard`, assert no
single feature exceeds a per-feature AUC ceiling (leakage screen).

---

## T7 — Drift schedule

Add a `drift:` config block: at day D, shift the fraud-type mix, change merchant
category mix, apply an amount inflation rate. Campaign phases already exist
(`initialize_campaign`); this extends the idea to the population.

**Acceptance:** test splits the window in half and asserts a KS statistic between halves
above a configured threshold, confirming drift is actually present.

---

## T8 — Measurement harness (build alongside T1, not last)

`scrip audit` subcommand, or a `metrics` module, that reports three layers:

- **statistical**: per-column marginals, p50/p99, skew, pairwise correlation drift
- **behavioral**: IET percentiles and autocorrelation, session/burst counts, trigger
  rates for a canonical velocity rule set (≥3 txns/1h, ≥5 txns/24h, amount z-score > 3,
  same-merchant repeat within 10 min), graph fan-out / clustering / triangles
- **leakage screen**: single-feature AUC per column against the label

This is the scoreboard for every task above, and it's the thing that makes the work
defensible to someone who doesn't trust synthetic data.

**Acceptance:** `cargo run -- audit --input-dir out` prints all three sections; the
velocity-rule trigger rates appear for both classes; no network or external data needed.

---

## Do not do

- Don't make the model's AUC the target. A dataset where one feature separates the
  classes is a broken dataset, not a good result.
- Don't resample to a balanced prior. Keep the realistic rate and let the consumer
  decide; document the prior-shift correction instead.
- Don't leave a field that describes a mechanism that doesn't run. Either implement
  `burst_session` for legitimate bursts too, or delete the column. Same for
  `card.status` if T2 isn't done.
- Don't put finance semantics in `src/core`.
- Don't regenerate the golden silently. Every fingerprint change needs the metric table
  in the commit message.

## Order

T0 → **T8 (minimum: the behavioral metrics half)** → (finish the `scrip-core` extraction if
it's in flight) → T1 → T2 → T3 → T4 → T5 → T6 → T7.

T8 moves ahead of T1 because the original T1 acceptance criteria were only discovered to be
mathematically incompatible by simulating the model first — which is exactly the job of the
audit command. Build the scoreboard, then change the model against it. Full T8 (leakage
screen, statistical layer) can wait; the behavioral metrics cannot.

T1 is still the highest-value behavior change: it fixes inter-event timing, burst
structure, velocity-rule rates, and gives card-testing sequences for free. T3 is second-most
valuable if the model will be evaluated on labels rather than on ground truth.
