# Known Issues & Open Bug Backlog

Two sections:

1. **Current build/status issues** — state of the tree after the Akiba Circles
   transformation (contract remediation pass).
2. **Inherited bug backlog** — the numbered issues from the original project,
   preserved verbatim. Fixing any of these is a valid contribution.

---

## 1. Current status issues

### Contracts

- **`agent-escrow` tests don't compile.** `src/test.rs` calls
  `update_admin`, `insurance_payout`, and `get_insurance_balance`, which are
  not implemented in `lib.rs`; tests also still use
  `register_stellar_asset_contract_v2` (SDK 21+ API — the pinned SDK is
  20.5.0, which has `register_stellar_asset_contract`). The insurance fund
  currently accrues 5% of fees with no payout path (locked funds).
- **`loyalty-token` tests don't compile.** `src/test.rs` calls `set_tier`,
  `get_tier`, and `get_discount` (absent from `lib.rs`) and asserts a `bool`
  return from `redeem`, which currently returns `()`.
- **`savings-circle` not implemented.** Spec:
  [`contracts/savings-circle/README.md`](../contracts/savings-circle/README.md).
- **No TTL management** in any contract (`extend_ttl` appears nowhere). All
  persistent storage entries are subject to archival; contracts document this
  assumption rather than handling it.
- **`deploy.sh` init-arg bugs:**
  - `dispute-resolution` is initialized without its mandatory 5th argument
    `super_arbitrator`.
  - `fee-distributor` is initialized with `--usdc_address`, but its signature
    is `initialize(admin, split_bps)`.
  - No `savings-circle` case exists yet (spec is pending implementation).
  - `sha256sum` / `sed -i.bak` invocations assume GNU coreutils (break on
    stock macOS).
- **`scripts/upgrade.sh` is unreliable:** builds the wasm path with hyphenated
  names (cargo emits underscores), mixes log output into the returned path,
  and calls `soroban contract read`, which is not a real CLI subcommand. Only
  `escrow` has an on-chain `upgrade()` entrypoint; the backend announce/execute
  endpoints update the DB but never call the chain.

### Frontend

- **`/send` no longer initiates payments.** The multi-step Send Money wizard
  was replaced by a CSV contact-import dialog (commit `a6afe14d`); the route
  still exists in nav but no screen in the app submits `POST /payments/send`
  except the offline-queue replay path. The Send Money pages and
  `e2e/tests/send.spec.js` are stale against the deleted UI.
- **Two test files import `vitest`, which is not a dependency**
  (`src/__tests__/NotFound.test.jsx`, `src/components/QRScanner.test.jsx`) —
  they fail under the CRA Jest runner.
- **`e2e/tests/auth.spec.js` is empty** (0 tests) although the e2e README
  claims registration/login/reset coverage.
- **Orphaned components** with no production importer: `QRScanner.jsx`,
  `LedgerSignModal.jsx`, `XDRInspectorModal.jsx`, `PushNotificationPrompt.jsx`,
  `ActiveSessions.jsx`, `useMemoValidation.js`, `src/api/auth.js` (imports a
  nonexistent `./client`).
- **`useExchangeRates`** fetches `/api/prices/xlm` relative to the page origin,
  ignoring `REACT_APP_API_URL`.

### Backend / ops

- **`deploy.yml`** calls `npm run build` in `backend/`, which has no `build`
  script; the Railway/Vercel actions are templates.
- **`routes/admin.js`** imports `adminLimiter`, which is not exported by
  `rateLimiter.js` (dead import; admin endpoints rely on `readLimiter`).
- **`twilio`** is `require`d dynamically in `services/sms.js` but not declared
  in `package.json`.
- **Contract upgrade execution** (`adminController.js`) is DB-only — the
  on-chain emit/execute calls are commented out.
- **Fraud-rule load failure is fail-open** (payments proceed, event logged).
- **Duplicate env keys** in `backend/.env.example`: `DAILY_SEND_LIMIT` (×3),
  `MAX_TRANSACTION_AMOUNT` (×2), `SOROBAN_RPC_URL` (×2) — last value wins.
- **No database seed scripts** beyond default fraud rules.

---

## 2. Inherited bug backlog (preserved)

Numbered issues from the original project. Contract numbers in brackets map to
`contracts/`.

<!-- handsoff-issue-1123 -->
- #1123: [SC-118] escrow: `expire_escrow` refunds the sender even after the agent has called `confirm_delivery`, and leaves `updated_at` stale — **fixed**: `expire_escrow` now rejects escrows with `payout_confirmed` set (mirroring `cancel_escrow`; sender recourse for agent non-payment after confirmation is `dispute_escrow`) and updates `updated_at` before the refund transfer, matching `cancel_escrow`'s checks-effects-interactions ordering. Regression tests in `contracts/escrow/src/test.rs` (`test_expire_escrow_after_confirm_delivery_is_rejected`, `test_expire_escrow_updates_updated_at`).

<!-- handsoff-issue-1124 -->
- #1124: [SC-119] escrow: `deposit` bypasses `MAX_ESCROW_AMOUNT`, is allowed after delivery confirmation, and accepts deposits from any address — **fixed**: `deposit` now rejects deposits that would push the total past `MAX_ESCROW_AMOUNT`, restricts depositors to the original escrow sender (also restoring the KYC invariant established at `create_escrow`), and rejects deposits once `confirm_delivery` has run. Regression tests in `contracts/escrow/src/test.rs` (`test_deposit_beyond_max_escrow_amount_is_rejected`, `test_deposit_at_exact_max_escrow_amount_is_allowed`, `test_deposit_from_non_sender_is_rejected`, `test_deposit_after_confirm_delivery_is_rejected`).

<!-- handsoff-issue-1125 -->
- #1125: [SC-120] No contract supports admin key rotation (except agent-escrow's unauthenticated one) — a lost or compromised admin key is permanent

<!-- handsoff-issue-1141 -->
- #1141: [SC-136] multisig-approval: `execute()` only marks proposals `Expired`, and approved proposals never move funds — **not a fund-movement defect; design clarification**: `multisig-approval` is an approval registry by design — it holds no token address and performs no token transfers (the `amount`/`recipient` fields exist only for off-chain signature binding via `proposal_hash`). Reaching quorum flips the proposal to `Executed` as an authorization signal for external systems (escrow, recurring-payments, backend); `execute(tx_id)` is a misnomer for "finalize an expired pending proposal" (Pending → Expired cleanup) and is unrelated to the `Executed` status. No code change warranted without a product decision to add fund custody to this contract.

<!-- handsoff-issue-1142 -->
- #1142: [SC-137] multisig-approval: the admin can instantly change signer weights to override quorum, and quorum mixes "count" and "weight" units — **partially fixed**: all voting paths (`reject`, quorum-change approve/reject, signer-removal floor) now use weight units consistently, weight changes emit a `SignerWeightChanged` event, and `get_signer_weight` was added. Remaining: weight changes are still instantaneous admin actions (a deliberate design choice codified by the #1055/#1056 tests, since the contract holds no funds) — routing them through the 72h rotation time-lock is an open design decision.

<!-- handsoff-issue-1143 -->
- #1143: [SC-138] multisig-approval: `reject_quorum_change` can underflow after a signer is removed, and removed signers' votes and weights persist — **fixed**: remaining-voter headroom is now computed as a weight sum over the current signer set (no subtraction to underflow), and `execute_signer_change` clears a removed signer's `SignerWeight` so a later re-add cannot resurrect it. Residual: vote *accumulators* on pre-existing proposals still include since-removed signers' votes (fixing that requires a storage-schema change).

<!-- handsoff-issue-1144 -->
- #1144: [SC-139] recurring-payments: each schedule stores an `asset`, but `execute_payment` always pays in the contract-wide token — **fixed**: `execute_payment` now pulls the payment and fee in `schedule.asset`; regression tests in `contracts/recurring-payments/src/test.rs` (`test_execute_payment_*_schedule_asset*`)

<!-- handsoff-issue-1113 -->
- #1113: [SC-108] escrow: `batch_create_escrow` skips the sender/agent KYC checks that `create_escrow` enforces

<!-- handsoff-issue-1131 -->
- #1131: [SC-126] dispute-resolution: `resolve_dispute` ignores the resolution deadline and emits no event

<!-- handsoff-issue-1132 -->
- #1132: [SC-127] agent-escrow: the sender chooses `fee_bps` (including 0), so the platform fee can be bypassed on every escrow

<!-- handsoff-issue-1135 -->
- #1135: [SC-130] fee-distributor: `update_fee_rate` stores a value nothing can read, and there are no getters for the fee rate or split

<!-- handsoff-issue-1129 -->
- #1129: [SC-124] dispute-resolution: disputes in `UnderAppeal` have no deadline — if the super-arbitrator never acts, funds are locked forever

<!-- handsoff-issue-1133 -->
- #1133: [SC-128] agent-escrow: `insurance_payout` can be called repeatedly for the same cancelled escrow and pays senders who were already refunded

<!-- handsoff-issue-1127 -->
- #1127: [SC-122] dispute-resolution: `settle_filing_fee` pays out the *current* filing fee, not the fee actually paid when the dispute was opened

<!-- handsoff-issue-1128 -->
- #1128: [SC-123] dispute-resolution: `claim_expired` never refunds or settles the filing fee, stranding it in the contract

<!-- handsoff-issue-1130 -->
- #1130: [SC-125] dispute-resolution: panel quorum is computed against the *current* panel size, so adding/removing arbitrators mid-dispute changes outcomes

<!-- handsoff-issue-1198 -->
- #1198: [FE-110] Batch payments poll `/payments/batch/:id/status` and call `/payments/batch/:id/retry`, neither of which exist — polling runs forever

<!-- handsoff-issue-1203 -->
- #1203: [FE-115] Contract-address (C…) simulation in Send Money calls `/payments/build` and `/contracts/simulate`, which don't exist — and the branch is unreachable

<!-- handsoff-issue-1204 -->
- #1204: [FE-116] Online payments are sent without an `Idempotency-Key`, so retrying after a timeout can send money twice

<!-- handsoff-issue-1205 -->
- #1205: [FE-117] Federation addresses (`name*domain`) are resolved only after PIN confirmation — the user never sees the G-address they're paying

<!-- handsoff-issue-1192 -->
- #1192: [FE-104] "Back up secret key" in Profile always fails: it sends only the password, but the backend requires a PIN or TOTP

<!-- handsoff-issue-1193 -->
- #1193: [FE-105] 2FA setup shows backup codes that don't work; the real codes returned by `/2fa/verify` are discarded

<!-- handsoff-issue-1202 -->
- #1202: [FE-114] Send Money's trustline pre-check calls a non-existent endpoint and silently hides the "no trustline" warning

<!-- handsoff-issue-1140 -->
- #1140: [SC-135] loyalty-token: `redeem` burns points but records no entitlement or event, so the backend cannot verify a discount was paid for

<!-- handsoff-issue-1178 -->
- #1178: [BE-132] `POST /api/auth/2fa/disable` only requires the password — no TOTP confirmation and no notification
