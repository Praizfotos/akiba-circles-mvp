# Changelog

All notable changes to this repository (formerly *AfriPay*, now **Akiba Circles**)
are documented in this file.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).
Breaking changes are marked **[BREAKING]**.

---

## [Unreleased]

### Fixed — `escrow` expire_escrow respects delivery confirmation (SC-118, issue #1123)

- `contracts/escrow/src/lib.rs` — `expire_escrow` now:
  - rejects escrows whose delivery has been confirmed (previously anyone
    could refund the sender after `confirm_delivery` once expiry passed,
    robbing the agent of the release they were owed; sender recourse for
    agent non-payment is `dispute_escrow`);
  - updates `updated_at` to the expiry timestamp before the refund transfer
    (previously left stale, so `cleanup_escrow`'s retention window started
    from the wrong moment), matching `cancel_escrow`'s
    checks-effects-interactions ordering.
- 2 regression tests added (`test_expire_escrow_after_confirm_delivery_is_rejected`,
  `test_expire_escrow_updates_updated_at`); both fail against the previous
  implementation. 74/74 green.

### Fixed — `escrow` deposit hardened (SC-119, issue #1124)

- `contracts/escrow/src/lib.rs` — `deposit` now:
  - rejects any deposit that would push the escrow total past
    `MAX_ESCROW_AMOUNT` (previously only enforced at `create_escrow`, so a
    pending escrow could be inflated without bound);
  - only accepts deposits from the original escrow sender (previously any
    address could top up any pending escrow, also bypassing the KYC check
    performed on the sender at creation);
  - rejects deposits after `confirm_delivery` (status stays `Pending` so
    release can proceed, but the deal is closed — further top-ups would
    inflate the agent's release with no sender recourse since `cancel_escrow`
    is already blocked).
- 4 regression tests added (`test_deposit_beyond_max_escrow_amount_is_rejected`,
  `test_deposit_at_exact_max_escrow_amount_is_allowed`,
  `test_deposit_from_non_sender_is_rejected`,
  `test_deposit_after_confirm_delivery_is_rejected`); the three rejection
  tests fail against the previous implementation. 72/72 green.

### Fixed — `multisig-approval` quorum voting unified on weight units (SC-137/SC-138, issues #1142/#1143)

- `contracts/multisig-approval/src/lib.rs`:
  - `reject` and `reject_quorum_change`/`approve_quorum_change` now count
    **weight**, not votes — previously a set of low-weight signers could never
    reject a proposal whose quorum was expressed in weight, and count-based
    remaining headroom could wrongly kill a still-reachable quorum change.
  - `reject_quorum_change` remaining-voter headroom is computed as a weight
    sum over the current signer set — the old
    `approvers.len() - approvals - rejections` subtraction underflowed (u32)
    once signer removals shrank the list below votes cast (SC-138).
  - `execute_signer_change` (Remove) checks **weight** reachability (allowing
    safe removals the old headcount check wrongly blocked at the `len ==
    quorum` floor) and clears the removed signer's `SignerWeight` so a re-add
    cannot resurrect a stale weight.
  - `propose_quorum_change` validates `new_quorum` against total signer
    weight, not signer count.
  - `set_signer_weight` now emits `SignerWeightChanged`; new
    `get_signer_weight` getter. (Weight changes remain instantaneous admin
    actions — deliberate design, documented in KNOWN_ISSUES.)
- 10 regression tests added (`test_reject_counts_weight_not_votes`,
  `test_reject_quorum_change_remaining_uses_weight`,
  `test_reject_quorum_change_no_underflow_after_removal`,
  `test_removed_signer_weight_cleared_on_rotation`,
  `test_remove_signer_checks_weight_reachability`,
  `test_remove_signer_allows_weight_safe_removal_at_count_floor`,
  `test_propose_quorum_change_validates_against_total_weight`,
  `test_propose_quorum_change_above_total_weight_panics`,
  `test_set_signer_weight_emits_event`,
  `test_get_signer_weight_defaults_and_updates`); all fail against the
  previous implementation where applicable. 91/91 green.

### Fixed — `recurring-payments` pays in the schedule's asset (SC-139, issue #1144)

- `contracts/recurring-payments/src/lib.rs` — `execute_payment` now pulls the
  payment and the platform fee in the schedule's `asset` token instead of the
  contract-wide `initialize` token (which remains the default only for the
  deprecated `authorize_recurring` alias). Previously a schedule created with a
  non-default asset either paid the wrong token or failed every window with
  "insufficient allowance" until auto-cancel. Four regression tests added
  (`test_execute_payment_*`); 55/55 green. (Note: issue #1144 had been marked
  closed by a README-only commit — the defect was still present in code.)

### Added — `savings-circle` core contract (Akiba Circles centerpiece)

- `contracts/savings-circle/` — rotating savings (esusu/tontine) contract on
  pinned `soroban-sdk` 20.5.0: `create_circle` / `join_circle` (pulls
  contribution + collateral, credits cycle 0) / `start_circle` / `contribute` /
  permissionless `rollover` (drops defectors, forfeits their collateral into
  the pot, deterministic rotation, fee accrual) / `cancel_circle` /
  `claim_collateral` / `withdraw_fees` + getters. `#[contracterror]` enum,
  `symbol_short!` events, CEI ordering. **41/41 unit tests green**; wasm
  builds for `wasm32v1-none`. Design decisions documented in the crate README.
- `contracts/deploy.sh` — `savings-circle` deploy + init case
  (`CONTRACT=savings-circle`, env `CIRCLE_FEE_BPS` optional, default 0).
- `backend/.env.example` — `SAVINGS_CIRCLE_CONTRACT_ID`.
- `webapp/` — new **Next.js 14 + TypeScript** dApp for the Circles product:
  wallet connect via `@creit.tech/stellar-wallets-kit` (Freighter et al.),
  Soroban RPC transaction building via `@stellar/stellar-sdk`, circle
  list/detail/create/contribute/rollover/claim pages, Testnet default,
  env-configurable contract IDs. `npm run build` clean.

### Added — product transformation to Akiba Circles

The project direction is now **on-chain rotating savings circles**
(esusu/tontine): see the root `README.md`. The engineering foundation,
Git history, and contributor attribution from the AfriPay project are
preserved unchanged.

- `contracts/savings-circle/README.md` — full implementation spec for the new
  core contract (data model, API, events, `#[contracterror]` design,
  invariants, test requirements) awaiting an implementer.
- `docs/ROADMAP.md` — phased contribution plan + good-first-issues.
- `docs/KNOWN_ISSUES.md` — current build/status issues plus the inherited
  numbered bug backlog (moved verbatim from the old README's "Handsoff notes").
- Root `README.md` and `contracts/README.md` rewritten for the new product
  and current contract status.

### Fixed — contract remediation pass (all under pinned `soroban-sdk` 20.5.0)

Prior to this change **0 of 9** contract crates compiled their test suites and
two crates (`fee-distributor`, `kyc-attestation`) had truncated, uncompilable
sources. Now **7 of 9 crates are green — 378 unit tests passing**:

- `fee-distributor` — **rebuilt** the truncated ~8.8k chars (rest of
  `deposit_fee`, `withdraw_fees`, `get_accumulated_fees`,
  `distribute_agent_pool`, pause/unpause, getters) from the committed test
  suite. 71 tests pass.
- `kyc-attestation` — **rebuilt** truncated regions (`assert_admin`,
  `revoke_batch`, `has_ever_attested`, `get_attestation`, `is_verified`,
  `get_highest_tier`); added `Debug`/`PartialEq` derives to `KycTier`;
  fixed test API mismatches (by-value → by-ref client args, `bytes!` macro
  usage, non-zero base ledger timestamp for `!= 0` sentinels). 31 tests pass.
- `savings-vault` — fixed the stuck-yield bug: `distribute_yield` now credits
  per-user `YieldAccrued` proportionally, so `claim_yield` can actually pay.
  51 tests pass.
- `escrow` — replaced the uncompilable `Address::from_contract_id` hack in
  `is_kyc_verified` with explicit user-address threading. 68 tests pass.
- `dispute-resolution` — replaced non-existent `env.caller()` with
  invoker-contract auth (`escrow_contract.require_auth()`); deleted the
  orphaned `types.rs`. 56 tests pass.
- `multisig-approval` — replaced non-existent `crypto::Hash` import with the
  20.x `sha256` API; added the missing `ToXdr` import. 81 tests pass.
- `recurring-payments` — removed the uncompilable deprecated
  `authorize_recurring` zero-address path. 51 tests pass.
- All crates: replaced SDK-21+ `register_stellar_asset_contract_v2` test calls
  with the 20.x `register_stellar_asset_contract`; added a vendored
  `ethnum` 1.5.0 compatibility patch (modern rustc rejects the pinned
  upstream's `transmute`).

### Not fixed (open for contribution — see `docs/ROADMAP.md`)

- `agent-escrow` and `loyalty-token`: test suites still don't compile —
  missing functions act as specs for first-time contributors.
- `deploy.sh` init-arg bugs for `dispute-resolution` / `fee-distributor`.
- Frontend `/send` flow, empty e2e auth spec, and other inherited issues —
  listed in `docs/KNOWN_ISSUES.md`. (Inherited issue #1112, "escrow
  `batch_create_escrow` missing closing brace", is resolved as a side effect
  of the escrow fix.)

### Security — backend routes (`backend/src/routes/wallet.js`)

#### Fixed: `GET /api/wallet/signers` bypassed admin/owner authorization (BE-001, issue #948)

`GET /api/wallet/signers` was registered twice — once with no authorization
middleware, then again correctly guarded by `isAdminOrOwner()`. Express
dispatches to the first matching registration for a given method+path, so the
unprotected copy always won and the protected copy was dead code. Any
authenticated user (not just an account admin/owner) could call the endpoint.

- **Introduced:** 2026-03-28 (`3ef2d463`, initial multisig support), when the
  route existed only in its unprotected form.
- **Regressed to dead-code duplicate:** 2026-05-31 (`3e08fc16`), when
  `isAdminOrOwner()` protection was added as a second registration instead of
  replacing the first.
- **Fixed:** 2026-08-29 — removed the unprotected registration; only the
  `isAdminOrOwner()`-guarded route is now reachable.
- **Exposure window:** 2026-03-28 through 2026-08-29.
- A duplicate-route CI check (`backend/scripts/check-duplicate-routes.js`)
  now fails the build on any (method, path) registered more than once in a
  router file, so this class of bug can't silently regress again.

### Changed — agent-escrow contract (`contracts/agent-escrow`)

#### **[BREAKING]** Standardized Soroban event topic scheme

All contract events now use a **two-element topic vector**:

```
topic[0] = Symbol("AgentEscrow")   ← contract name, used as Horizon filter prefix
topic[1] = Symbol("<EventName>")   ← specific event name (see table below)
```

Previously every event used a single-element topic vector, e.g. `(Symbol("EscrowCreated"),)`.
Any off-chain subscriber filtering on the old single-element topics will stop receiving events
after this contract is redeployed and must be updated to match the new two-element scheme.

| Event | Old topic (single) | New topic[0] | New topic[1] |
|---|---|---|---|
| Escrow created | `EscrowCreated` | `AgentEscrow` | `EscrowCreated` |
| Payout confirmed | `PayoutConfirmed` | `AgentEscrow` | `EscrowConfirmed` |
| Escrow cancelled | `EscrowCancelled` | `AgentEscrow` | `EscrowCancelled` |
| Admin override | `AdminOverride` | `AgentEscrow` | `AdminOverride` |

#### **[BREAKING]** Updated event data payloads

| Struct | Field changes |
|---|---|
| `EvtEscrowCreated` (was `EvtCreated`) | `fee_bps` field **removed**; all other fields unchanged |
| `EvtEscrowConfirmed` (was `EvtCompleted`) | `agent: Address` field **added** |
| `EvtEscrowCancelled` (was `EvtCancelled`) | `sender: Address` field **added** |

#### **[BREAKING]** `PayoutConfirmed` event renamed to `EscrowConfirmed`

Subscribers listening for `PayoutConfirmed` must update to `EscrowConfirmed`.

---

### Fixed — backend service (`backend/src/services/agentEscrow.js`)

- `confirmPayout` was calling the non-existent contract method `confirm_delivery`.
  It now correctly calls `confirm_payout`, matching the deployed contract ABI.

### Added — backend service (`backend/src/services/agentEscrow.js`)

- `ESCROW_EVENT_NAMES` — frozen constant object mapping logical names to the
  on-chain event name strings:
  ```js
  ESCROW_EVENT_NAMES.CREATED        // "EscrowCreated"
  ESCROW_EVENT_NAMES.CONFIRMED      // "EscrowConfirmed"
  ESCROW_EVENT_NAMES.CANCELLED      // "EscrowCancelled"
  ESCROW_EVENT_NAMES.ADMIN_OVERRIDE // "AdminOverride"
  ```
- `subscribeToEscrowEvents(eventName, onEvent, onError)` — opens a Horizon
  contract-event SSE stream filtered to `[AgentEscrow, eventName]` and invokes
  `onEvent(event)` for each matching event. Returns a `close()` function.

  ```js
  const { subscribeToEscrowEvents, ESCROW_EVENT_NAMES } = require('./services/agentEscrow');

  const stop = subscribeToEscrowEvents(
    ESCROW_EVENT_NAMES.CONFIRMED,
    (evt) => console.log('payout confirmed', evt),
    (err) => console.error('stream error', err),
  );
  // later…
  stop();
  ```

---

## Migration Guide

### Contract redeployment

The event schema changes require a **new contract deployment**. You cannot
upgrade event topics on an already-deployed contract without redeployment.

1. Build and deploy the updated contract:
   ```bash
   cd contracts/agent-escrow
   cargo build --target wasm32-unknown-unknown --release
   stellar contract deploy --wasm target/wasm32-unknown-unknown/release/agent_escrow_contract.wasm \
     --network testnet --source <admin-keypair>
   ```
2. Update `AGENT_ESCROW_CONTRACT_ID` in your `.env` to the new contract address.
3. Re-initialize the new contract:
   ```bash
   stellar contract invoke --id <new-contract-id> -- initialize \
     --admin <admin-address> \
     --usdc_address <usdc-sac-address> \
     --cancel_window_seconds 172800
   ```

### Updating Horizon event subscribers

Replace any existing single-element topic filter with the two-element scheme.

**Before (old single-element filter):**
```js
// Was listening for { topic: ["EscrowCreated"] }
server.contractEvents()
  .forContract(CONTRACT_ID)
  .stream({ onmessage: handler });
```

**After (new two-element filter via the helper):**
```js
const { subscribeToEscrowEvents, ESCROW_EVENT_NAMES } = require('./services/agentEscrow');

const stop = subscribeToEscrowEvents(ESCROW_EVENT_NAMES.CREATED, handler);
```

Or, if filtering manually via Horizon's `topic` query parameter:
```
GET /contract_events?contract_id=<id>&topic=AgentEscrow,EscrowCreated
```

### Updating event payload consumers

| If your code reads… | Change to… |
|---|---|
| `event.data.fee_bps` on `EscrowCreated` | Read `fee_bps` from `get_escrow()` instead |
| `PayoutConfirmed` event data | Use `EscrowConfirmed`; decode `agent`, `agent_amount`, `fee_amount` |
| `EscrowCancelled` event data | Decode added `sender` field |
