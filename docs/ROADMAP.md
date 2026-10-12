# Roadmap & Contribution Guide

Akiba Circles is being built **in the open**. The table below is the source of
truth for what exists and what needs contributors. Anything listed here is
available — claim it by opening an issue/PR.

## Phase status

| Phase | What | Status |
|---|---|---|
| 1 | Contract-layer remediation (rebuild truncated crates, SDK 20.5.0 alignment, yield-accounting fix) | ✅ **done** |
| 2 | `savings-circle` core contract | ✅ **done** — implemented, 41/41 tests green |
| 3 | Backend `/api/circles/*` (service, controller, routes, tests) | ⬜ not started |
| 4 | Frontend: Next.js + TypeScript dApp (`webapp/`) with wallet-kit | ⬜ not started |
| 5 | Testnet deployment + smoke test | ⬜ not started |
| 6 | Inherited bug backlog ([KNOWN_ISSUES.md](./KNOWN_ISSUES.md)) | ⬜ ongoing |

## Phase 2 — implement `savings-circle` ✅ done

Implemented in [`contracts/savings-circle/`](../contracts/savings-circle/):
rotating savings with collateral-forfeit drops, permissionless `rollover`,
fee accrual, and `claim_collateral`. 41/41 unit tests green; wasm builds for
`wasm32v1-none`. Design decisions (join credits cycle 0, `now + cycle_seconds`
deadline policy, completion at `< 2` active) are documented in the crate
README. This crate is the repo's reference for `#[contracterror]` handling.

Still open for contributors (related to the contract layer):

- Property/fuzz testing of the rotation math.
- A second token-denominated circle (multi-token support in `DataKey`).
- Gas/ledger-footprint benchmarking of `rollover` at `max_members = 100`.

## Phase 3 — backend circle API

Create `backend/src/services/savingsCircle.js` (Soroban RPC invocation, modeled
on `services/loyaltyToken.js`), a controller, and
`backend/src/routes/savingsCircles.js`, wired in `app.js`. Suggested surface:

```
GET    /api/circles                 list (status filter, pagination)
POST   /api/circles                 create + join-by-creator
GET    /api/circles/:id             circle detail + members
POST   /api/circles/:id/join        build allowance tx + join tx (custodial signing)
POST   /api/circles/:id/start
POST   /api/circles/:id/contribute
POST   /api/circles/:id/rollover    anyone; backend cron job should also call it
POST   /api/circles/:id/cancel
GET    /api/circles/my              caller's circles
```

Reuse: `authMiddleware`, `geoRestriction`, `services/stellar.js` key
encryption + tx queue, the `contractEventIndexer` job (add the circle contract
ID to `SOROBAN_CONTRACT_IDS`). Add `SAVINGS_CIRCLE_CONTRACT_ID` to
`backend/.env.example`. Follow the existing validator/Jest patterns.

## Phase 4 — frontend

The product frontend is a **new Next.js + TypeScript dApp** in `webapp/`
(non-custodial — users connect Freighter or another Stellar wallet via
`@creit.tech/stellar-wallets-kit`; the contract calls are made from the
browser with `@stellar/stellar-sdk`). It sits alongside the inherited
Create-React-App `frontend/` (legacy AfriPay UI, still functional for the
original flows). The Next.js app owns the Circles product surface:

- Circle list, create-circle wizard, circle detail (members, cycle
  countdown, contribute, rollover, claim collateral).
- Wallet connect/disconnect, network switch (Testnet default).
- Env-configurable contract IDs (`NEXT_PUBLIC_SAVINGS_CIRCLE_CONTRACT_ID`,
  `NEXT_PUBLIC_SOROBAN_RPC_URL`, `NEXT_PUBLIC_USDC_CONTRACT_ID`).

Open contribution opportunities in `webapp/`: i18n (`sw`/`ha`/`yo`/`fr`),
event feed from the contract indexer, circle chat/notifications, and design
polish. The legacy `frontend/` rebrand (chrome, locales, dead `/send` route)
remains a separate, optional track.

## Phase 5 — deployment

- `savings-circle` deploy+init is wired in `contracts/deploy.sh`
  (`CONTRACT=savings-circle`). The two broken init-arg cases
  (`dispute-resolution` / `fee-distributor`) still need fixes — see
  KNOWN_ISSUES.
- Deploy to **Stellar Testnet** only; record contract IDs in
  `deployments/testnet_contract_ids.json` (gitignored) and in the backend
  `.env`.
- Never commit secret keys or seed phrases.

## Quick wins (good first issues)

1. **`agent-escrow`**: implement `update_admin`, `insurance_payout`,
   `get_insurance_balance` in `lib.rs` per `src/test.rs`, and rename the
   tests' `register_stellar_asset_contract_v2` → `register_stellar_asset_contract`.
   The test suite is the spec; it is the only thing standing between this
   crate and a green `cargo test`.
2. **`loyalty-token`**: implement `set_tier`, `get_tier`, `get_discount`;
   make `redeem` return `bool`; add the approve race-guard the tests expect.
3. **`e2e/tests/auth.spec.js`**: it is empty — write registration/login/2FA
   flows against the existing Playwright setup.
4. **Fix the two vitest-importing frontend test files** (they break the CRA
   Jest run).
5. **`deploy.sh` init-arg bugs** (dispute-resolution, fee-distributor).
6. Anything numbered in [KNOWN_ISSUES.md](./KNOWN_ISSUES.md).

## Contribution norms

- Preserve Git history — ordinary commits on branches, no force-pushes, no
  history rewrites.
- Match each crate's existing style (panic messages, event shape, storage
  patterns) except where the `savings-circle` spec says otherwise.
- Every contract change ships with tests. "It compiles" is not done.
- Run before you push:
  - `cargo test --manifest-path contracts/<crate>/Cargo.toml`
  - `cd backend && npm test` (for backend changes)
  - `cd frontend && CI=true npm test` (for frontend changes)
  - `npm run lint` where applicable
- No secrets in commits. Ever.
