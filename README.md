# Akiba Circles — On-Chain Rotating Savings Circles for Africa

**Akiba Circles** brings the *esusu / tontine / stokvel / susu* — the rotating
savings circles that millions of African and diaspora households already use —
on-chain on **Stellar**, so the group no longer has to trust a human collector
with a bag of cash.

> **Status:** mid-transformation, built for contributors. Phase 1 (contract
> remediation) is done and green; the core `savings-circle` contract is
> [fully specified and waiting for an implementer](contracts/savings-circle/README.md).
> See [`docs/ROADMAP.md`](docs/ROADMAP.md). This project is community-built and
> is **not** affiliated with, or endorsed by, the Stellar Development
> Foundation or any ecosystem program.

---

## The problem

A rotating savings circle works like this: 10 people each contribute $100
every week; each week one person walks away with $1,000, cycling through the
group until everyone has received. It is one of the most common savings
mechanisms in Africa — and it has one structural flaw: **the collector holds
all the cash.** Collectors abscond, pay late, lose records, or "forget" whose
turn it is. Diaspora members participating from abroad have even less
visibility.

## The solution

The collector becomes a Soroban smart contract:

- Contributions (USDC) are pulled on-chain into the circle's account.
- The payout rotation is deterministic and auditable — join order, enforced
  by the contract, not memory.
- A member who misses a contribution is **dropped**: they forfeit their
  collateral to the pot, and the remaining members see it in their payout.
- Anyone can call `rollover` when a cycle deadline passes — no single keeper.
- The platform takes a small, configurable fee on pot payouts; nothing else.

The contract eliminates *collector risk* and enforces *schedule and rotation*.
Residual *member credit risk* (someone who receives their pot and then quits)
is mitigated by drop-and-forfeit — and, honestly, by the same social fabric
that makes these circles work today. We document that assumption rather than
pretend it away.

**Why Stellar/Soroban?** Sub-cent fees make weekly micro-contributions
economical; USDC is the de-facto settlement asset for African cross-border
value; and Soroban gives us deterministic on-chain enforcement of the rotation
state machine with Rust-grade testability. A classic (non-smart-contract)
Stellar rail cannot express per-cycle membership state; an off-chain database
cannot eliminate the collector.

## Who it's for

- Savings circle members and organizers (market associations, burial societies,
  family groups, salary earners' associations)
- Diaspora participants contributing to circles back home
- Developers building on Stellar who want a reusable group-coordination
  primitive

---

## Repository structure

| Path | What it is |
|---|---|
| `contracts/` | Independent Soroban crates (Rust/WASM). **`savings-circle/` is the new core — spec only.** See [`contracts/README.md`](contracts/README.md) for per-crate status. |
| `backend/` | Express + PostgreSQL API: auth (JWT/2FA/WebAuthn), wallets, payments, fraud/AML, compliance, cron jobs, ~209 routes |
| `frontend/` | React 18 (CRA) + Tailwind SPA; i18n (en/fr/sw/ha/yo); offline queue; service worker |
| `database/` | Bootstrap schema + 95 `node-pg-migrate` migrations |
| `e2e/` | Playwright suites (send/receive/history/user-journey; auth spec is an open task) |
| `docs/` | [`ROADMAP.md`](docs/ROADMAP.md) · [`KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md) · runbooks · security reports |
| `scripts/` | Integration verification helpers |

**Heritage:** this repository was formerly *AfriPay*, a custodial
cross-border remittance platform. Its engineering foundation — auth,
compliance, observability, CI, and seven of its nine smart contracts — is
preserved intact (with the original Git history and contributor attribution);
the product direction is now savings circles. Details of what changed:
[`CHANGELOG.md`](CHANGELOG.md).

## Architecture

| Layer | Technology |
|---|---|
| Contracts | Rust, `soroban-sdk` **20.5.0** (pinned — do not upgrade casually), wasm32 |
| Backend | Node 20, Express, `@stellar/stellar-sdk` v12 (Horizon + Soroban RPC), PostgreSQL 16, Redis/BullMQ |
| Frontend | React 18, CRA, Tailwind, react-router 6, axios, i18next |
| Wallets | Server-custodial today (AES-256-encrypted keys, PIN/TOTP gated export); browser-wallet migration is future work |
| Infra | Docker Compose, GitHub Actions CI, Prometheus/Grafana, Sentry |

### Smart contracts

| Crate | Purpose | `cargo test` |
|---|---|---|
| `savings-circle` | **Rotating savings circles — Akiba Circles core** | 📋 [spec](contracts/savings-circle/README.md) |
| `escrow` | 3-party remittance escrow, batch, KYC/dispute hooks | ✅ 68 |
| `dispute-resolution` | On-chain arbitration + appeals + quorum | ✅ 56 |
| `fee-distributor` | Fee accumulation & split routing | ✅ 71 |
| `kyc-attestation` | Tiered KYC status (hash-only, no PII) | ✅ 31 |
| `savings-vault` | Time-locked savings + yield | ✅ 51 |
| `recurring-payments` | Allowance-based recurring transfers | ✅ 51 |
| `multisig-approval` | Weighted multisig + signer rotation | ✅ 81 |
| `agent-escrow` | Agent-mediated escrow + insurance fund | ❌ open task |
| `loyalty-token` | SEP-41 loyalty token | ❌ open task |

**Total: 378 contract unit tests passing** across seven crates. The two ❌
crates have complete test suites that act as specs — implementing the missing
functions is an ideal first contribution ([good first issues](docs/ROADMAP.md)).

---

## Getting started

### Prerequisites

- Docker + Docker Compose **or** Node 20, PostgreSQL 16, Redis
- Rust stable + `rustup target add wasm32-unknown-unknown` (contracts)
- `stellar` CLI (deployment)

### Quick start (Docker)

```bash
cp backend/.env.example backend/.env       # fill in JWT_SECRET, ENCRYPTION_KEY, …
cp frontend/.env.example frontend/.env
docker compose up --build
# frontend: http://localhost:3000   backend: http://localhost:5000
```

Database migrations run automatically on backend boot
(`npm run migrate`), or manually:

```bash
cd backend && npm run migrate
```

### Contracts

```bash
cargo build --release --target wasm32-unknown-unknown --manifest-path contracts/escrow/Cargo.toml
cargo test --manifest-path contracts/escrow/Cargo.toml        # repeat per crate
```

### Tests

```bash
cd backend  && npm test          # Jest unit + integration (STELLAR_NETWORK=mock in CI)
cd frontend && CI=true npm test  # CRA Jest
npm run test:e2e                 # Playwright (needs running stack)
```

CI (`.github/workflows/ci.yml`) additionally gates on duplicate-route checks,
migration-prefix checks, and per-crate WASM builds.

### Key environment variables

| Variable | Where | Notes |
|---|---|---|
| `DATABASE_URL`, `JWT_SECRET`, `ENCRYPTION_KEY` | backend | required; `ENCRYPTION_KEY` = 32 chars |
| `STELLAR_NETWORK` | backend/frontend | `testnet` / `mainnet` |
| `SOROBAN_RPC_URL` | backend | Soroban RPC endpoint |
| `SAVINGS_CIRCLE_CONTRACT_ID` | backend | *(planned — Phase 3)* |
| `ESCROW_CONTRACT_ID`, `AGENT_ESCROW_CONTRACT_ID`, … | backend | per-contract IDs from deployment |
| `REACT_APP_API_URL`, `REACT_APP_STELLAR_NETWORK` | frontend | see `frontend/.env.example` |

Full lists: `backend/.env.example`, `frontend/.env.example`. **Never commit
real `.env` files, seed phrases, or secret keys.**

---

## Deploying contracts to Stellar Testnet

```bash
export SOROBAN_SECRET_KEY='S...'          # testnet secret key, never commit it
export SOROBAN_RPC_URL='https://soroban-testnet.stellar.org'
bash contracts/deploy.sh --network testnet
```

The script builds each crate to optimized WASM, deploys, initializes
atomically, and writes contract IDs to
`deployments/testnet_contract_ids.json` and `contracts/.env` (both
gitignored). Known bugs in the init step are tracked in
[KNOWN_ISSUES.md](docs/KNOWN_ISSUES.md) — test the initialize step manually
for `dispute-resolution` and `fee-distributor` until they're fixed. After
deployment, copy the contract IDs into `backend/.env` and restart.

---

## Security & limitations — read this

- **Trust model:** the `savings-circle` contract removes collector risk; it
  does not remove member credit risk. Collateral + drop-on-default mitigate;
  they do not eliminate. Do not deploy funds you cannot afford to socialize.
- **Custodial wallets:** the inherited backend holds user keys
  (AES-encrypted). This is appropriate for the current product stage but is
  **not** self-custody; a Freighter/Wallet-Kit migration is on the long-term
  roadmap.
- **Contracts are unaudited.** 378 passing unit tests ≠ an audit. Get a
  Soroban security review before mainnet.
- **SDK pinned at 20.5.0** with a vendored `ethnum` compatibility patch for
  modern rustc; upgrading is a coordinated, repo-wide change.
- **No TTL/rent management** in any contract yet — persistent entries can be
  archived; this is documented, not handled.
- **Compliance inherited from AfriPay** (KYC thresholds, AML screening, geo
  blocking, clawback) applies to the payments side; circle-specific compliance
  policy is undefined — an open design question for the community.

See [`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md) for the full open-bug list
and [`docs/security/`](docs/security/) for historical fix reports.

---

## Contributing

**We want contributors.** The product is deliberately being built in phases —
[`docs/ROADMAP.md`](docs/ROADMAP.md) lists exactly what's open, including
good-first-issues (implement the two broken contract crates, write the empty
e2e auth spec, fix `deploy.sh`, build the circle API/UI).

Norms: preserve Git history; ordinary reviewable commits; every contract change
ships with tests; match existing crate style; run the relevant test suites
before pushing; no secrets, ever.

The inherited bug backlog is preserved in
[`docs/KNOWN_ISSUES.md`](docs/KNOWN_ISSUES.md) — numbered issues welcome as
PRs.

## Resources

- [Stellar Developer Docs](https://developers.stellar.org)
- [Soroban Docs](https://soroban.stellar.org)
- [Soroban by Example](https://soroban-by-example.org)
- [Stellar Expert](https://stellar.expert)
- [Stellar Discord](https://discord.gg/stellar)

## Project history

This repository preserves the complete Git history of the AfriPay project
(1,100+ commits, all contributors attributed). See [`CHANGELOG.md`](CHANGELOG.md)
for the transformation log and [`CHANGES.md`](CHANGES.md) plus
`docs/reports/` for the inherited engineering reports.

## License

MIT
