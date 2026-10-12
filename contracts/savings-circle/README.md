# savings-circle — the Akiba Circles core contract

> **Status: implemented. 41/41 unit tests green** (`cargo test`). Built with
> `soroban-sdk = 20.5.0` (repo-wide pin — see [`contracts/README.md`](../README.md)).

A **rotating savings circle** (esusu / tontine / stokvel / susu) where a group
of members contributes a fixed amount every cycle and one member receives the
full pot each cycle, rotating deterministically through the group.

Today these circles are run by a human collector who holds everyone's cash and
can abscond, mis-time payouts, or "forget" whose turn it is. This contract is
the collector: it holds contributions on-chain, enforces the payout rotation,
and drops members who miss a contribution (their collateral is forfeited into
the pot, compensating the members who did pay).

## Design decisions (and why)

- **Join credits cycle 0.** `join_circle` pulls `contribution + collateral` and
  marks the member as having contributed cycle 0. There is no separate payment
  required at `start_circle` — no double-charge.
- **No enrollment window.** The circle stays `Open` and joinable until any
  member/creator calls `start_circle`. Simplest model that works on-chain; a
  time-bound enrollment can be layered on later.
- **Deadline policy: `now + cycle_seconds`, not drift.** Each rollover sets
  `cycle_deadline = env.ledger().timestamp() + cycle_seconds`. Late
  permissionless rollovers shift the schedule honestly rather than stacking
  catch-up cycles.
- **Rotation math.** After dropping defectors, the recipient is the member at
  index `current_cycle % active_count` among *still-active* members in join
  order. Fair under drops; the recipient of the drop-forfeited collateral is
  therefore the same cycle's recipient.
- **Completion at `< 2` active members.** With 1 member left there is nothing to
  rotate — the circle is `Completed` and the survivor claims their collateral
  (any residual pot was paid out in the final rollover).
- **Fees accrue to `DataKey::Fees`**, withdrawable only by the initialized
  admin (`withdraw_fees`). The admin cannot touch member funds or collateral —
  the entire point of the product.
- **CEI discipline.** All state (drops, fee accrual, `total_received`, pot
  reset, cycle increment, completion) is written before the single token
  transfer out of `rollover`.

## Data model

```rust
#[contracttype] pub enum CircleStatus { Open, Active, Completed, Cancelled }

#[contracttype] pub struct Circle {
    pub id: u64,
    pub creator: Address,
    pub name: String,                 // 1..=64 bytes
    pub contribution: i128,            // > 0, per member per cycle
    pub collateral: i128,              // >= 0; posted at join, forfeited on drop
    pub cycle_seconds: u64,            // >= 60
    pub max_members: u32,              // 2..=100
    pub status: CircleStatus,
    pub current_cycle: u32,
    pub cycle_deadline: u64,           // ledger timestamp of current cycle end
    pub active_count: u32,             // members not dropped
    pub created_at: u64,
    pub pot: i128,                     // this cycle's accumulated contributions
}

#[contracttype] pub struct MemberMeta {
    pub joined_at: u64,
    pub slot: u32,          // join-order index
    pub active: bool,       // false once dropped
    pub total_contributed: i128,
    pub total_received: i128,
}
```

Storage: Persistent for everything (no `extend_ttl` anywhere in this repo —
entries can be archived; documented assumption, same as sibling crates).

| Key | Value |
|---|---|
| `DataKey::Admin` | `Address` |
| `DataKey::Token` | `Address` (any SAC token; in prod, USDC) |
| `DataKey::FeeBps` | `u32` (platform fee on each pot payout, `<= 1000`) |
| `DataKey::Fees` | `i128` (accrued platform fees) |
| `DataKey::Counter` | `u64` (circle id sequence, 1-based) |
| `DataKey::Circle(u64)` | `Circle` |
| `DataKey::Members(u64)` | `Vec<Address>` (join order = rotation order) |
| `DataKey::Member(u64, Address)` | `MemberMeta` |
| `DataKey::Contributed(u64, u32, Address)` | `bool` (circle, cycle, member) |

## Public API

All panics use `panic_with_error!` with:

| Error | Code |
|---|---|
| `NotInitialized` | 0 |
| `AlreadyInitialized` | 1 |
| `Unauthorized` | 2 |
| `InvalidParams` | 3 |
| `CircleNotFound` | 4 |
| `InvalidStatus` | 5 |
| `AlreadyMember` | 6 |
| `NotMember` | 7 |
| `AlreadyContributed` | 8 |
| `MaxMembersReached` | 9 |
| `NotEnoughMembers` | 10 |
| `TooEarly` | 11 |
| `NothingToWithdraw` | 12 |
| `AlreadyClaimed` | 13 |

1. **`initialize(env, admin, token, fee_bps)`** — one-time; re-init guard.
2. **`create_circle(env, creator, name, contribution, cycle_seconds, max_members, collateral) -> u64`**
   — `creator.require_auth()`; validates all bounds; status `Open`; returns id.
3. **`join_circle(env, member, circle_id)`** — only `Open`; not already a
   member; room available. Pulls `contribution + collateral` via
   `token.transfer_from` (member must approve this contract first) and credits
   the member for cycle 0.
4. **`start_circle(env, caller, circle_id)`** — any member or creator; `Open`
   with `>= 2` members; status `Active`; `cycle_deadline = now + cycle_seconds`.
5. **`contribute(env, member, circle_id)`** — only `Active`; member active;
   not yet contributed this cycle; pulls `contribution`; adds to `pot`.
6. **`rollover(env, caller, circle_id)`** — permissionless (`caller` only
   binds the invocation). Requires `Active` and `now >= cycle_deadline`. Order:
   1. Drop every member who has not contributed this cycle: `active = false`,
      forfeited `collateral` moves into the pot.
   2. If `active_count == 0`: status `Completed`; return (nothing paid).
   3. Recipient = active members (join order) at `current_cycle % active_count`.
   4. `fee = pot * fee_bps / 10_000` → accrue to `Fees`; transfer `pot - fee`
      to recipient; `total_received += pot - fee`.
   5. `pot = 0`; `current_cycle += 1`; `cycle_deadline = now + cycle_seconds`.
   6. If `active_count < 2`: status `Completed`.
7. **`cancel_circle(env, creator, circle_id)`** — creator only; only `Open`;
   refunds every joined member `contribution + collateral`; status `Cancelled`.
8. **`withdraw_fees(env, admin, to)`** — admin only; transfers accrued fees.
9. **`claim_collateral(env, member, circle_id)`** — only `Completed`; only
   non-dropped members; one-shot; returns the posted collateral.
10. **Getters**: `get_circle`, `get_members(id, start, limit)` (limit capped
    at 50), `get_member`, `get_fees`, `get_token`, `get_counter` (highest id
    issued — lets clients enumerate circles 1..=counter).

### Events

Single `Symbol` topic + struct data (repo style): `created`, `joined`,
`started`, `contrib`, `dropped`, `rollover`, `cancelled`, `collat`
(collateral claimed), `fees` — emitted via `symbol_short!` (SDK 20's 9-char
symbol cap).

## Invariants (tested in `src/test.rs`)

1. **Conservation**: for a completed circle, `sum(contributed) + forfeited
   collateral == sum(received) + residual pot (always 0 at completion)`.
2. A member who never contributes cannot receive the pot.
3. A defector's collateral lands in the cycle pot and reaches that cycle's
   recipient.
4. `rollover` before `cycle_deadline` → `TooEarly`.
5. Double contribution in one cycle → `AlreadyContributed`.
6. Re-init, non-admin fee withdrawal, join-after-start, join-full-circle,
   cancel-after-start — all rejected with the right error code.
7. With N members and no drops, slot `i` receives in cycles `i`, `i+N`, `i+2N`, …
8. Everyone defaults in a cycle → all dropped, `Completed`, contract balance =
   collateral only (no contributions stranded — pot is paid to the cycle's
   recipient when at least one member pays; when nobody pays, pot was 0).
9. Fee math: `fee_bps = 0` → recipient gets full pot; `fee_bps = 100` → 1%.
10. Dropped members cannot `claim_collateral`; survivors can, exactly once;
    contract drains to zero.

Run: `cargo test --manifest-path contracts/savings-circle/Cargo.toml`

## Security notes

- CEI: all effects before the single transfer in `rollover`/`join`/`contribute`.
- Every argument validated at the boundary (`amount > 0`, `cycle_seconds >= 60`,
  `name` 1..=64 bytes, `max_members` 2..=100, `fee_bps <= 1000`).
- `transfer_from` only ever pulls from the authenticated caller — no arbitrary
  pulls.
- The admin can only `withdraw_fees`. No path touches member funds.
- Native tests need a non-zero base ledger timestamp (see `src/test.rs` —
  `li.timestamp = 100`); sentinel-style fields break at t=0 (kyc-attestation
  lesson).
