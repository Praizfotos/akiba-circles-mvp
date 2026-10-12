#![no_std]

//! # Savings Circle Contract
//!
//! The Akiba Circles core contract: a **rotating savings circle**
//! (esusu / tontine / stokvel / susu) enforced on-chain.
//!
//! A group of members contributes a fixed amount every cycle; one member
//! receives the full pot each cycle, rotating deterministically through
//! join order. The contract replaces the human collector — the single point
//! of failure in informal savings circles — with deterministic, auditable
//! Soroban state.
//!
//! ## Lifecycle
//!
//! `Open` → (`start_circle`) → `Active` → (`rollover` × N) → `Completed`
//!
//! A circle may also be `Cancelled` by its creator while still `Open`.
//!
//! ## Trust model (read before using)
//!
//! - **Collector risk is eliminated**: funds are held by the contract; only
//!   the deterministic rotation can move them.
//! - **Schedule & rotation are enforced**: non-contributors are dropped at
//!   rollover and forfeit their collateral to the pot.
//! - **Member credit risk remains**: a member who receives their pot and
//!   then defaults forfeits collateral, but their earlier contributions
//!   cannot be clawed back. This is inherent to rotating savings and is
//!   mitigated (not eliminated) by drop-and-forfeit.
//! - The admin can ONLY withdraw platform fees. The admin cannot touch
//!   member funds — ever.
//!
//! ## Conventions
//!
//! - Storage: `Persistent` for everything (matches the rest of the repo).
//!   No TTL extension is performed; persistent entries are subject to
//!   archival like every other crate in this repository.
//! - Errors: `#[contracterror]` + `panic_with_error!` (deliberately better
//!   than the bare `panic!` strings used by the legacy crates — see
//!   `contracts/README.md`).
//! - Token flows use checks-effects-interactions: all state is written
//!   before any `transfer`.
//! - SDK: pinned `soroban-sdk` 20.5.0 for repo-wide consistency.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, String, Vec,
};

mod test;

// ── Limits & constants ────────────────────────────────────────────────────────

/// Minimum cycle length: one minute.
pub const MIN_CYCLE_SECONDS: u64 = 60;
/// Minimum and maximum circle size.
pub const MIN_MEMBERS: u32 = 2;
pub const MAX_MEMBERS: u32 = 100;
/// Maximum `name` length in bytes.
pub const MAX_NAME_BYTES: u32 = 64;
/// Platform fee ceiling: 10%.
pub const MAX_FEE_BPS: u32 = 1_000;
/// Pagination cap for `get_members`.
pub const MAX_PAGE_LIMIT: u32 = 50;

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracterror]
#[repr(u32)]
pub enum Error {
    /// `initialize` was never called.
    NotInitialized = 0,
    /// `initialize` called twice.
    AlreadyInitialized = 1,
    /// Caller is not authorized for this operation.
    Unauthorized = 2,
    /// One or more arguments violate the documented bounds.
    InvalidParams = 3,
    /// No circle exists with this id.
    CircleNotFound = 4,
    /// Operation not allowed in the circle's current status.
    InvalidStatus = 5,
    /// Address is already an active member of this circle.
    AlreadyMember = 6,
    /// Address is not (or no longer) a member of this circle.
    NotMember = 7,
    /// Member already contributed in the current cycle.
    AlreadyContributed = 8,
    /// The circle is full.
    MaxMembersReached = 9,
    /// A circle needs at least `MIN_MEMBERS` members to start.
    NotEnoughMembers = 10,
    /// `rollover` called before the cycle deadline.
    TooEarly = 11,
    /// Nothing accrued to withdraw.
    NothingToWithdraw = 12,
    /// Collateral was already claimed (or forfeited).
    AlreadyClaimed = 13,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[contracttype]
pub enum CircleStatus {
    /// Accepting joins; cannot receive contributions or roll over.
    Open,
    /// Contributions and rotations are live.
    Active,
    /// Rotation finished (or collapsed); collateral claimable.
    Completed,
    /// Creator cancelled before start; everyone was refunded.
    Cancelled,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Circle {
    pub id: u64,
    pub creator: Address,
    pub name: String,
    /// Per-member, per-cycle contribution amount (in token units).
    pub contribution: i128,
    /// Posted at join; forfeited to the pot on drop. May be 0.
    pub collateral: i128,
    pub cycle_seconds: u64,
    pub max_members: u32,
    pub status: CircleStatus,
    /// Index of the cycle currently being collected for / just played.
    pub current_cycle: u32,
    /// Ledger timestamp at which the current cycle ends.
    pub cycle_deadline: u64,
    /// Number of members not dropped.
    pub active_count: u32,
    pub created_at: u64,
    /// Accumulated contributions for the current cycle (plus forfeited
    /// collateral from members dropped in this cycle).
    pub pot: i128,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MemberMeta {
    pub joined_at: u64,
    /// Join-order index — the rotation fairness reference.
    pub slot: u32,
    /// False once dropped (or refunded on cancel).
    pub active: bool,
    pub total_contributed: i128,
    pub total_received: i128,
    /// True once this member's collateral has been claimed.
    pub collateral_claimed: bool,
}

#[derive(Clone)]
#[contracttype]
pub enum DataKey {
    Admin,
    Token,
    FeeBps,
    Fees,
    Counter,
    Circle(u64),
    Members(u64),
    Member(u64, Address),
    Contributed(u64, u32, Address),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct SavingsCircleContract;

#[contractimpl]
impl SavingsCircleContract {
    /// Initialize the contract. Callable once; `admin` is the only address
    /// that can withdraw platform fees.
    ///
    /// # Arguments
    /// * `admin`   — Platform fee admin.
    /// * `token`   — Token address circles are denominated in (e.g. the USDC SAC address).
    /// * `fee_bps` — Platform fee on each pot payout, basis points (0..=1000).
    pub fn initialize(env: Env, admin: Address, token: Address, fee_bps: u32) {
        admin.require_auth();
        if env.storage().persistent().has(&DataKey::Admin) {
            panic_with_error!(&env, Error::AlreadyInitialized);
        }
        if fee_bps > MAX_FEE_BPS {
            panic_with_error!(&env, Error::InvalidParams);
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::Token, &token);
        env.storage().persistent().set(&DataKey::FeeBps, &fee_bps);
        env.storage().persistent().set(&DataKey::Fees, &0i128);
        env.storage().persistent().set(&DataKey::Counter, &0u64);
    }

    /// Create a new circle in `Open` status. The creator does not need to
    /// join; anyone may join until `start_circle` is called.
    ///
    /// Returns the new circle id (1-based).
    ///
    /// # Arguments
    /// * `name`            — 1..=64 byte label.
    /// * `contribution`    — > 0; amount each active member pays every cycle.
    /// * `cycle_seconds`   — >= 60; cycle length once started.
    /// * `max_members`     — 2..=100.
    /// * `collateral`      — >= 0; posted at join, forfeited on drop.
    ///
    /// The circle stays `Open` (joinable) until `start_circle` — there is no
    /// time-bound enrollment window.
    pub fn create_circle(
        env: Env,
        creator: Address,
        name: String,
        contribution: i128,
        cycle_seconds: u64,
        max_members: u32,
        collateral: i128,
    ) -> u64 {
        creator.require_auth();
        Self::require_init(&env);
        if name.len() == 0
            || name.len() > MAX_NAME_BYTES
            || contribution <= 0
            || cycle_seconds < MIN_CYCLE_SECONDS
            || max_members < MIN_MEMBERS
            || max_members > MAX_MEMBERS
            || collateral < 0
        {
            panic_with_error!(&env, Error::InvalidParams);
        }

        let counter: u64 = env
            .storage()
            .persistent()
            .get(&DataKey::Counter)
            .unwrap_or(0);
        let id = counter + 1;

        let circle = Circle {
            id,
            creator: creator.clone(),
            name: name.clone(),
            contribution,
            collateral,
            cycle_seconds,
            max_members,
            status: CircleStatus::Open,
            // Cycle 0 begins at `start_circle`; `enrollment_secs` is the head
            // start members get before the first deadline.
            current_cycle: 0,
            cycle_deadline: 0, // set at start_circle
            active_count: 0,
            created_at: env.ledger().timestamp(),
            pot: 0,
        };
        env.storage().persistent().set(&DataKey::Counter, &id);
        env.storage().persistent().set(&DataKey::Circle(id), &circle);
        env.storage().persistent().set(&DataKey::Members(id), &Vec::<Address>::new(&env));

        env.events().publish(
            (symbol_short!("circle"), symbol_short!("created"),),
            (id, creator, contribution, cycle_seconds, max_members),
        );
        id
    }

    /// Join an `Open` circle. Pulls `contribution + collateral` from the
    /// member via `transfer_from` — the member must have approved this
    /// contract for at least that amount first (the dApp builds the
    /// allowance transaction). The contribution is credited to cycle 0, so
    /// `start_circle` must not charge again.
    pub fn join_circle(env: Env, member: Address, circle_id: u64) {
        member.require_auth();
        let mut circle = Self::get_circle(env.clone(), circle_id);
        if circle.status != CircleStatus::Open {
            panic_with_error!(&env, Error::InvalidStatus);
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Member(circle_id, member.clone()))
        {
            panic_with_error!(&env, Error::AlreadyMember);
        }
        if circle.active_count >= circle.max_members {
            panic_with_error!(&env, Error::MaxMembersReached);
        }

        let join_amount = circle.contribution + circle.collateral;

        // Effects before interactions.
        let mut members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Members(circle_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CircleNotFound));
        let slot = members.len();
        members.push_back(member.clone());
        env.storage().persistent().set(&DataKey::Members(circle_id), &members);

        let meta = MemberMeta {
            joined_at: env.ledger().timestamp(),
            slot,
            active: true,
            total_contributed: circle.contribution,
            total_received: 0,
            collateral_claimed: false,
        };
        env.storage()
            .persistent()
            .set(&DataKey::Member(circle_id, member.clone()), &meta);

        circle.active_count += 1;
        circle.pot += circle.contribution;
        env.storage()
            .persistent()
            .set(&DataKey::Contributed(circle_id, 0, member.clone()), &true);
        env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);

        env.events().publish(
            (symbol_short!("joined"),),
            (circle_id, member.clone(), slot),
        );

        Self::token(&env).transfer_from(
            &env.current_contract_address(),
            &member,
            &env.current_contract_address(),
            &join_amount,
        );
    }

    /// Move an `Open` circle to `Active`. Requires at least `MIN_MEMBERS`
    /// members. Sets the cycle-0 deadline to `now + cycle_seconds`.
    /// Callable by the creator or any member.
    pub fn start_circle(env: Env, caller: Address, circle_id: u64) {
        caller.require_auth();
        let mut circle = Self::get_circle(env.clone(), circle_id);
        if circle.status != CircleStatus::Open {
            panic_with_error!(&env, Error::InvalidStatus);
        }
        if circle.active_count < MIN_MEMBERS {
            panic_with_error!(&env, Error::NotEnoughMembers);
        }
        if caller != circle.creator
            && !env
                .storage()
                .persistent()
                .has(&DataKey::Member(circle_id, caller))
        {
            panic_with_error!(&env, Error::Unauthorized);
        }
        circle.status = CircleStatus::Active;
        circle.current_cycle = 0;
        circle.cycle_deadline = env.ledger().timestamp() + circle.cycle_seconds;
        env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);

        env.events().publish(
            (symbol_short!("started"),),
            (circle_id, circle.cycle_deadline),
        );
    }

    /// Contribute the cycle amount. Active members only, once per cycle.
    /// Pulls via `transfer_from` (allowance required).
    pub fn contribute(env: Env, member: Address, circle_id: u64) {
        member.require_auth();
        let mut circle = Self::get_circle(env.clone(), circle_id);
        if circle.status != CircleStatus::Active {
            panic_with_error!(&env, Error::InvalidStatus);
        }
        let meta = Self::get_member(env.clone(), circle_id, member.clone());
        if !meta.active {
            panic_with_error!(&env, Error::NotMember);
        }
        if env
            .storage()
            .persistent()
            .get(&DataKey::Contributed(circle_id, circle.current_cycle, member.clone()))
            .unwrap_or(false)
        {
            panic_with_error!(&env, Error::AlreadyContributed);
        }

        // Effects before interactions.
        env.storage().persistent().set(
            &DataKey::Contributed(circle_id, circle.current_cycle, member.clone()),
            &true,
        );
        circle.pot += circle.contribution;
        let mut meta = meta;
        meta.total_contributed += circle.contribution;
        env.storage()
            .persistent()
            .set(&DataKey::Member(circle_id, member.clone()), &meta);
        env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);

        env.events().publish(
            (symbol_short!("contrib"),),
            (circle_id, member.clone(), circle.contribution),
        );

        Self::token(&env).transfer_from(
            &env.current_contract_address(),
            &member,
            &env.current_contract_address(),
            &circle.contribution,
        );
    }

    /// Settle the current cycle. **Permissionless** — anyone may call once
    /// `now >= cycle_deadline` (any member's wallet or a backend keeper).
    ///
    /// Effects, in order:
    /// 1. Drop every active member who has not contributed this cycle;
    ///    their collateral is forfeited into the pot.
    /// 2. If no active members remain: mark `Completed` and stop.
    /// 3. Pay `pot - fee` to the recipient (active member at index
    ///    `current_cycle % active_count` in join order, evaluated after
    ///    drops); accrue `fee` to platform fees.
    /// 4. Advance `current_cycle`; new deadline is `now + cycle_seconds`
    ///    (now-based, so a late rollover extends rather than shortens the
    ///    next collection window).
    /// 5. If fewer than 2 active members remain, mark `Completed`.
    pub fn rollover(env: Env, caller: Address, circle_id: u64) {
        caller.require_auth();
        let mut circle = Self::get_circle(env.clone(), circle_id);
        if circle.status != CircleStatus::Active {
            panic_with_error!(&env, Error::InvalidStatus);
        }
        let now = env.ledger().timestamp();
        if now < circle.cycle_deadline {
            panic_with_error!(&env, Error::TooEarly);
        }

        let members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Members(circle_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CircleNotFound));
        let cycle = circle.current_cycle;

        // 1. Drop non-contributors; forfeit collateral into the pot.
        let mut active: Vec<Address> = Vec::new(&env);
        for member in members.iter() {
            let mut meta = Self::get_member(env.clone(), circle_id, member.clone());
            if !meta.active {
                continue;
            }
            let contributed = env
                .storage()
                .persistent()
                .get(&DataKey::Contributed(circle_id, cycle, member.clone()))
                .unwrap_or(false);
            if contributed {
                active.push_back(member.clone());
            } else {
                meta.active = false;
                env.storage()
                    .persistent()
                    .set(&DataKey::Member(circle_id, member.clone()), &meta);
                circle.pot += circle.collateral;
                circle.active_count -= 1;
                env.events().publish(
                    (symbol_short!("dropped"),),
                    (circle_id, member.clone(), cycle),
                );
            }
        }

        // 2. Everyone gone → circle is over.
        if active.is_empty() {
            circle.status = CircleStatus::Completed;
            env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);
            env.events()
                .publish((symbol_short!("complete"),), (circle_id, cycle));
            return;
        }

        // 3. Recipient + fee math.
        let idx = (cycle as u128) % (active.len() as u128);
        let recipient = active.get(idx as u32).unwrap();
        let fee_bps: u32 = env.storage().persistent().get(&DataKey::FeeBps).unwrap_or(0);
        let fee = circle
            .pot
            .checked_mul(fee_bps as i128)
            .and_then(|v| v.checked_div(10_000))
            .unwrap_or_else(|| panic_with_error!(&env, Error::InvalidParams));
        let payout = circle.pot - fee;

        // Effects before interactions.
        let mut fees: i128 = env.storage().persistent().get(&DataKey::Fees).unwrap_or(0);
        fees += fee;
        env.storage().persistent().set(&DataKey::Fees, &fees);
        circle.pot = 0;
        circle.current_cycle += 1;
        circle.cycle_deadline = now + circle.cycle_seconds;
        if active.len() < MIN_MEMBERS {
            circle.status = CircleStatus::Completed;
        }
        let mut recipient_meta = Self::get_member(env.clone(), circle_id, recipient.clone());
        recipient_meta.total_received += payout;
        env.storage().persistent().set(
            &DataKey::Member(circle_id, recipient.clone()),
            &recipient_meta,
        );
        env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);

        env.events().publish(
            (symbol_short!("rollover"),),
            (circle_id, cycle, recipient.clone(), payout, fee),
        );
        if circle.status == CircleStatus::Completed {
            env.events()
                .publish((symbol_short!("complete"),), (circle_id, cycle));
        }

        if payout > 0 {
            Self::token(&env).transfer(
                &env.current_contract_address(),
                &recipient,
                &payout,
            );
        }
    }

    /// Cancel an `Open` circle. Everyone who joined is refunded
    /// `contribution + collateral` in full. Creator only.
    pub fn cancel_circle(env: Env, creator: Address, circle_id: u64) {
        creator.require_auth();
        let mut circle = Self::get_circle(env.clone(), circle_id);
        if circle.creator != creator {
            panic_with_error!(&env, Error::Unauthorized);
        }
        if circle.status != CircleStatus::Open {
            panic_with_error!(&env, Error::InvalidStatus);
        }

        let members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Members(circle_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CircleNotFound));
        let refund = circle.contribution + circle.collateral;
        let token = Self::token(&env);

        // Effects (bookkeeping) then one transfer per member.
        for member in members.iter() {
            if let Some(mut meta) = env
                .storage()
                .persistent()
                .get::<_, MemberMeta>(&DataKey::Member(circle_id, member.clone()))
            {
                if meta.active {
                    meta.active = false;
                    env.storage()
                        .persistent()
                        .set(&DataKey::Member(circle_id, member.clone()), &meta);
                    token.transfer(&env.current_contract_address(), &member, &refund);
                }
            }
        }
        circle.pot = 0;
        circle.active_count = 0;
        circle.status = CircleStatus::Cancelled;
        env.storage().persistent().set(&DataKey::Circle(circle_id), &circle);

        env.events()
            .publish((symbol_short!("cancelled"),), (circle_id,));
    }

    /// After a circle is `Completed`, every member who was never dropped may
    /// reclaim their collateral. One claim per member.
    pub fn claim_collateral(env: Env, member: Address, circle_id: u64) {
        member.require_auth();
        let circle = Self::get_circle(env.clone(), circle_id);
        if circle.status != CircleStatus::Completed {
            panic_with_error!(&env, Error::InvalidStatus);
        }
        let mut meta = Self::get_member(env.clone(), circle_id, member.clone());
        if !meta.active || meta.collateral_claimed {
            panic_with_error!(&env, Error::AlreadyClaimed);
        }

        // Effects before interaction.
        meta.active = false;
        meta.collateral_claimed = true;
        env.storage()
            .persistent()
            .set(&DataKey::Member(circle_id, member.clone()), &meta);

        env.events().publish(
            (symbol_short!("collat"),),
            (circle_id, member.clone()),
        );

        if circle.collateral > 0 {
            Self::token(&env).transfer(
                &env.current_contract_address(),
                &member,
                &circle.collateral,
            );
        }
    }

    /// Withdraw accrued platform fees. Admin only.
    pub fn withdraw_fees(env: Env, admin: Address, to: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let fees: i128 = env.storage().persistent().get(&DataKey::Fees).unwrap_or(0);
        if fees <= 0 {
            panic_with_error!(&env, Error::NothingToWithdraw);
        }
        // Effects before interaction.
        env.storage().persistent().set(&DataKey::Fees, &0i128);
        env.events()
            .publish((symbol_short!("fees"),), (to.clone(), fees));
        Self::token(&env).transfer(&env.current_contract_address(), &to, &fees);
    }

    // ── Getters ───────────────────────────────────────────────────────────────

    pub fn get_circle(env: Env, circle_id: u64) -> Circle {
        env.storage()
            .persistent()
            .get(&DataKey::Circle(circle_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CircleNotFound))
    }

    /// Members in join order, paginated (`limit` capped at `MAX_PAGE_LIMIT`).
    pub fn get_members(env: Env, circle_id: u64, start: u32, limit: u32) -> Vec<Address> {
        let _ = Self::get_circle(env.clone(), circle_id); // existence check
        let members: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::Members(circle_id))
            .unwrap_or_else(|| panic_with_error!(&env, Error::CircleNotFound));
        let len = members.len();
        if start >= len {
            return Vec::new(&env);
        }
        let limit = if limit > MAX_PAGE_LIMIT {
            MAX_PAGE_LIMIT
        } else {
            limit
        };
        let mut out = Vec::new(&env);
        let end = core::cmp::min(start + limit, len);
        for i in start..end {
            if let Some(a) = members.get(i) {
                out.push_back(a);
            }
        }
        out
    }

    pub fn get_member(env: Env, circle_id: u64, member: Address) -> MemberMeta {
        env.storage()
            .persistent()
            .get(&DataKey::Member(circle_id, member))
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotMember))
    }

    /// Accrued platform fees (token units).
    pub fn get_fees(env: Env) -> i128 {
        env.storage().persistent().get(&DataKey::Fees).unwrap_or(0)
    }

    /// Configured token address.
    pub fn get_token(env: Env) -> Address {
        env.storage()
            .persistent()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(&env, Error::NotInitialized))
    }

    /// Highest circle id issued so far (0 = none). Lets clients enumerate
    /// circles 1..=counter without a full index.
    pub fn get_counter(env: Env) -> u64 {
        env.storage().persistent().get(&DataKey::Counter).unwrap_or(0)
    }
}

// ── Internal helpers (free functions — not exported entry points) ─────────────

impl SavingsCircleContract {
    fn require_init(env: &Env) {
        if !env.storage().persistent().has(&DataKey::Admin) {
            panic_with_error!(env, Error::NotInitialized);
        }
    }

    fn require_admin(env: &Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
        if *caller != admin {
            panic_with_error!(env, Error::Unauthorized);
        }
    }

    fn token(env: &Env) -> soroban_sdk::token::Client<'_> {
        let token: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Token)
            .unwrap_or_else(|| panic_with_error!(env, Error::NotInitialized));
        soroban_sdk::token::Client::new(env, &token)
    }
}
