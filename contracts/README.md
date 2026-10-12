# Smart Contracts

Soroban smart contracts for **Akiba Circles** (and the inherited payment
foundation this repository was built on).

## Status

Every crate is pinned to **`soroban-sdk = "20.5.0"`** — this is deliberate.
Do not upgrade the SDK in one crate without a repo-wide decision; several
crates were originally written against SDK 21/22 APIs and that mismatch is
what broke them.

| Contract | Description | `cargo test` |
|---|---|---|
| `savings-circle/` | **Rotating savings circles — the Akiba Circles core contract** | ✅ **implemented** — 41/41 tests green ([docs](./savings-circle/README.md)) |
| `escrow/` | Three-party (sender/recipient/agent) trustless escrow, batch creation, KYC/dispute hooks | ✅ 68 passed |
| `dispute-resolution/` | Three-party on-chain arbitration with appeals and quorum voting | ✅ 56 passed |
| `fee-distributor/` | Platform fee accumulation, split routing, agent pool, pause | ✅ 71 passed |
| `kyc-attestation/` | Tiered on-chain KYC status via SHA-256 hash (no raw PII) | ✅ 31 passed |
| `savings-vault/` | Time-locked savings vault with interest/yield distribution | ✅ 51 passed |
| `recurring-payments/` | User-authorized recurring transfers (allowance-based, non-custodial) | ✅ 51 passed |
| `multisig-approval/` | Weighted multisig proposals, quorum changes, signer rotation timelock | ✅ 81 passed |
| `agent-escrow/` | Agent-mediated escrow variant with insurance fund | ❌ tests don't compile — **[good first issue](../docs/ROADMAP.md)** |
| `loyalty-token/` | SEP-41 loyalty token with tiered redemption | ❌ tests don't compile — **[good first issue](../docs/ROADMAP.md)** |

`agent-escrow` and `loyalty-token` are intentionally left open: their
`src/test.rs` suites are complete and act as the specification — the missing
functions (`update_admin`, `insurance_payout`, `get_insurance_balance` /
`set_tier`, `get_tier`, `get_discount`, plus `redeem`'s return type) just need
to be implemented in `lib.rs`. Fixing them is an ideal first contribution.

## Layout

Each contract is an **independent crate** (no workspace): its own
`Cargo.toml`, `src/lib.rs`, `src/test.rs`. CI
(`.github/workflows/ci.yml`) globs `contracts/*/Cargo.toml`, builds each to
`wasm32-unknown-unknown`, and runs `cargo test`.

Generated artifacts (`target/`, `Cargo.lock`, `test_snapshots/`, `*.wasm`,
`deployments/`) are gitignored — never commit them.

## Building

```bash
cargo build --release --target wasm32-unknown-unknown --manifest-path contracts/<crate>/Cargo.toml
cargo test --manifest-path contracts/<crate>/Cargo.toml
```

Prerequisites: Rust (stable), `rustup target add wasm32-unknown-unknown`,
and the `stellar` CLI for deployment.

## Deployment

```bash
export SOROBAN_SECRET_KEY='S...'
bash contracts/deploy.sh --network testnet
```

`deploy.sh` builds, optimizes, deploys, and **immediately initializes** each
contract (atomic deploy+init prevents initialization front-running), then
writes `deployments/testnet_contract_ids.json` and `contracts/.env`.

> Known gaps in `deploy.sh` are tracked in [`docs/KNOWN_ISSUES.md`](../docs/KNOWN_ISSUES.md)
> (wrong init args for `dispute-resolution` / `fee-distributor`).
> PRs fixing the script are welcome. (`savings-circle` deploy+init is wired.)

## Upgradeability

Only `escrow` exposes an on-chain `upgrade()` + `migrate()` pair
(`contracts/escrow/src/lib.rs`). `UPGRADING.md` describes the intended
timelock flow; the backend announce/execute endpoints exist but the on-chain
execution path is DB-only today — see `docs/KNOWN_ISSUES.md`.

## Security conventions

What reviewers expect to see in this repo:

- `require_auth()` on every privileged entry point; admin = stored-address
  equality check (see `require_admin` helpers).
- Checks-effects-interactions ordering before every token transfer.
- Argument validation at the boundary with predictable errors.
- No TTL extension anywhere yet — contracts document their archival
  assumption rather than pretending otherwise.
- `savings-circle` is the repo's reference for the modern convention:
  `#[contracterror]` + `panic_with_error!` instead of bare `panic!("...")`.
  New crates should copy its error-handling pattern.

## Resources

- [Soroban Docs](https://soroban.stellar.org)
- [Stellar Expert](https://stellar.expert)
- [savings-circle spec](./savings-circle/README.md)
- [Roadmap / contribution tasks](../docs/ROADMAP.md)

## License

MIT (see root README).
