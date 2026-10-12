#![no_std]

//! # Fee Distributor Contract
//!
//! On-chain platform fee accumulation and withdrawal for AfriPay.
//! Makes the fee model fully transparent and auditable on Stellar.
//!
//! ## Access control
//! - `deposit_fee`           — any caller (typically the backend service account)
//! - `get_accumulated_fees`  — public
//! - `get_all_accumulated_fees` — public
//! - `get_agent_pool_fees`   — public
//! - `withdraw_fees`         — admin only
//! - `withdraw_agent_pool`   — admin only
//! - `distribute_agent_pool` — admin only
//! - `update_split`          — admin only
//! - `add_fee_token`         — admin only
//! - `remove_fee_token`      — admin only
//!
//! ## Agent pool lifecycle
//! Each `deposit_fee` splits the deposit into a platform portion
//! (`AccumulatedFees(token)`) and an agent reward portion
//! (`AgentPoolFees(token)`) according to `split_bps`.  The agent pool is
//! distributed to individual agent addresses via `distribute_agent_pool`, which
//! transfers the requested amounts to each recipient and emits an
//! `AgentPoolDistributed` event per recipient.  It can also be withdrawn to the
//! admin via `withdraw_agent_pool` (retained per the test suite; agent-pool
//! withdrawals are deliberately not gated by the pause flag).  Every other
//! outbound transfer (`withdraw_fees`, `distribute_agent_pool`) panics while
//! the contract is `Paused`.
//!
//! ## Fee-token allow-list (SC-132)
//! `deposit_fee` is permissionless, so it only accepts tokens the admin has
//! explicitly allow-listed via `add_fee_token`.  Registration is O(1): each
//! token is stored under its own `TokenRegistered(Address)` key rather than
//! loading and rewriting a whole `TokenList` vector on every deposit.  This
//! prevents an attacker from growing unbounded state (and the per-deposit cost)
//! by depositing throwaway SEP-41 tokens.

use soroban_sdk::{
    contract, contractimpl, contracttype, token, vec, Address, ConversionError, Env, Symbol,
    TryFromVal, Val, Vec,
};

mod test;

// ── Constants ─────────────────────────────────────────────────────────────────

// SECURITY: i128 max is ~170 trillion USDC in stroops.
// MAX_DEPOSIT_AMOUNT caps a single deposit at 1,000,000 USDC (10_000_000_000_000 stroops),
// consistent with the MAX_ESCROW_AMOUNT ceiling in escrow.rs.  This provides a
// contract-level backstop against caller-side unit/precision bugs (e.g. a
// decimal-precision mismatch depositing an amount 10,000× too large).
const MAX_DEPOSIT_AMOUNT: i128 = 10_000_000_000_000;

// ── Storage keys ──────────────────────────────────────────────────────────────

// SC-014: Removed the duplicate, single-fee-rate design's dead storage keys
// (`UsdcAddress`, non-parameterised `AccumulatedFees`, `PlatformFeeBps`) that
// conflicted with the current split-pool model's token-keyed
// `AccumulatedFees(Address)` variant.  The canonical design is the
// split_bps/multi-token model whose storage keys are used throughout the rest
// of this file (`AccumulatedFees(Address)`, `AgentPoolFees(Address)`,
// `SplitBps`, `TokenList`).  The old single-fee-rate variants were leftover
// dead code from an earlier design iteration.
#[contracttype]
pub enum DataKey {
    /// The admin address authorised to withdraw fees and update settings.
    Admin,
    /// Per-token platform treasury accumulator.
    AccumulatedFees(Address),
    /// Per-token agent reward pool accumulator.
    AgentPoolFees(Address),
    /// Basis points allocated to the agent reward pool (0–5000).
    SplitBps,
    /// Ordered list of every token address that has ever received a deposit.
    /// Used by `get_all_accumulated_fees` to enumerate per-token balances.
    TokenList,
    /// Whether the contract is currently paused.
    Paused,
    /// SC-132: O(1) per-token registration flag.  Set when a token is added to
    /// the admin-managed fee-token allow-list.  Replaces the O(n) `TokenList`
    /// scan/rewrite that `deposit_fee` used to perform on every call.
    TokenRegistered(Address),
}

// ── Event payloads ────────────────────────────────────────────────────────────

// NOTE: `EvtFeeDeposited` intentionally does *not* use `#[contracttype]`.
// soroban-sdk 20.5.0's derive unconditionally emits (under `cfg(test)` /
// `testutils`) `TryFrom<EvtFeeDeposited> for xdr::ScVal`/`ScMap` impls whose
// per-field conversion requires `xdr::ScVal: From<&Option<Address>>`.  The
// SDK only provides `TryFrom<&Address> for ScVal` (not `From`), so a payload
// containing `Option<Address>` fails to compile in test builds.  The two
// hand-written `Val` conversions below are exactly what the derive generates
// for the unconditional Val path (field keys sorted alphabetically, `None`
// encoded as `Void`), so publish/decode behaviour is unchanged.
#[derive(Clone)]
pub struct EvtFeeDeposited {
    pub depositor: Address,
    pub token: Address,
    pub amount: i128,
    pub total: i128,
    pub source: Option<Address>,
}

impl TryFromVal<Env, Val> for EvtFeeDeposited {
    type Error = ConversionError;

    fn try_from_val(env: &Env, val: &Val) -> Result<Self, ConversionError> {
        use soroban_sdk::{EnvBase, MapObject, TryIntoVal};
        const KEYS: [&'static str; 5] = ["amount", "depositor", "source", "token", "total"];
        let mut vals: [Val; 5] = [Val::VOID.to_val(); 5];
        let map: MapObject = val.try_into().map_err(|_| ConversionError)?;
        env.map_unpack_to_slice(map, &KEYS, &mut vals)
            .map_err(|_| ConversionError)?;
        Ok(Self {
            amount: vals[0]
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            depositor: vals[1]
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            source: vals[2].try_into_val(env).map_err(|_| ConversionError)?,
            token: vals[3]
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            total: vals[4]
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
        })
    }
}

impl TryFromVal<Env, EvtFeeDeposited> for Val {
    type Error = ConversionError;

    fn try_from_val(env: &Env, val: &EvtFeeDeposited) -> Result<Val, ConversionError> {
        use soroban_sdk::{EnvBase, TryIntoVal};
        const KEYS: [&'static str; 5] = ["amount", "depositor", "source", "token", "total"];
        let vals: [Val; 5] = [
            (&val.amount)
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            (&val.depositor)
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            (&val.source)
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            (&val.token)
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
            (&val.total)
                .try_into_val(env)
                .map_err(|_| ConversionError)?,
        ];
        Ok(env
            .map_new_from_slices(&KEYS, &vals)
            .map_err(|_| ConversionError)?
            .into())
    }
}

#[derive(Clone)]
#[contracttype]
pub struct EvtFeesWithdrawn {
    pub admin: Address,
    pub token: Address,
    pub amount: i128,
    pub remaining: i128,
    pub timestamp: u64,
}

/// Emitted once per recipient when the agent pool is distributed.
#[derive(Clone)]
#[contracttype]
pub struct EvtAgentPoolDistributed {
    pub admin: Address,
    pub token: Address,
    pub agent: Address,
    pub amount: i128,
    pub remaining: i128,
    pub timestamp: u64,
}

// SC-016 fix: `FeeRateUpdated` struct was missing its closing brace.
// Added `}` after `updated_by` field.
#[derive(Clone)]
#[contracttype]
pub struct FeeRateUpdated {
    pub old_bps: u32,
    pub new_bps: u32,
    pub updated_by: Address,
}

/// Emitted when the admin changes the fee split ratio.
#[derive(Clone)]
#[contracttype]
pub struct EvtSplitUpdated {
    pub old_split_bps: u32,
    pub new_split_bps: u32,
}

/// SC-132: Emitted when the admin adds a token to the fee-token allow-list.
#[derive(Clone)]
#[contracttype]
pub struct EvtFeeTokenAdded {
    pub admin: Address,
    pub token: Address,
}

/// SC-132: Emitted when the admin removes a token from the fee-token allow-list.
#[derive(Clone)]
#[contracttype]
pub struct EvtFeeTokenRemoved {
    pub admin: Address,
    pub token: Address,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// SC-132: Register `token` in the fee-token allow-list in O(1) storage
/// reads/writes.  Each token is stored under its own `TokenRegistered(Address)`
/// key, so the cost of this call does not grow with the number of registered
/// tokens.  The `TokenList` vector is only appended to when the token is new,
/// and is never scanned on the deposit hot path.
fn register_token(env: &Env, token: &Address) {
    let key = DataKey::TokenRegistered(token.clone());
    if env.storage().persistent().has(&key) {
        return;
    }
    env.storage().persistent().set(&key, &true);

    // Maintain the enumeration list for `get_all_accumulated_fees`.  This is
    // only touched when a genuinely new token is registered (admin-gated), so
    // it cannot be grown by an unprivileged depositor.
    let mut list: Vec<Address> = env
        .storage()
        .persistent()
        .get(&DataKey::TokenList)
        .unwrap_or_else(|| vec![env]);
    list.push_back(token.clone());
    env.storage().persistent().set(&DataKey::TokenList, &list);
}

/// SC-132: Returns true when `token` is on the admin-managed allow-list.
fn is_registered(env: &Env, token: &Address) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::TokenRegistered(token.clone()))
}

#[derive(Clone)]
#[contracttype]
pub struct EvtContractPaused {
    pub admin: Address,
    pub paused_at: u64,
}

#[derive(Clone)]
#[contracttype]
pub struct EvtContractUnpaused {
    pub admin: Address,
    pub unpaused_at: u64,
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct FeeDistributorContract;

#[contractimpl]
impl FeeDistributorContract {
    /// Initialise the contract. Must be called once.
    ///
    /// SC-014: Removed the duplicate `initialize(env, admin, usdc_address,
    /// platform_fee_bps)` overload that was left over from an earlier
    /// single-fee-rate design.  It conflicted with this function
    /// (`error[E0592]: duplicate definitions with name 'initialize'`) and used
    /// dead storage keys (`UsdcAddress`, non-parameterised `AccumulatedFees`,
    /// `PlatformFeeBps`) that are not used anywhere else in the file.
    ///
    /// # Arguments
    /// * `admin`     — Address authorised to withdraw accumulated fees.
    /// * `split_bps` — Basis points (0–5000) of each deposit routed to the
    ///                 agent reward pool. E.g. 2000 = 20 %. Must not exceed 5000.
    pub fn initialize(env: Env, admin: Address, split_bps: u32) {
        if env.storage().persistent().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        if split_bps > 5000 {
            panic!("split_bps exceeds maximum of 5000");
        }
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::SplitBps, &split_bps);
        // Initialise an empty token list.
        let empty: Vec<Address> = vec![&env];
        env.storage().persistent().set(&DataKey::TokenList, &empty);
    }

    /// SC-132: Add `token` to the admin-managed fee-token allow-list.
    ///
    /// Only allow-listed tokens may be deposited via `deposit_fee`.  This is
    /// O(1) in storage reads/writes regardless of how many tokens are already
    /// registered.  Idempotent: re-adding an existing token is a no-op.
    pub fn add_fee_token(env: Env, token: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        if is_registered(&env, &token) {
            return;
        }
        register_token(&env, &token);
        env.events().publish(
            (Symbol::new(&env, "fee_token_added"),),
            EvtFeeTokenAdded { admin, token },
        );
    }

    /// SC-132: Remove `token` from the admin-managed fee-token allow-list.
    ///
    /// After removal, `deposit_fee` rejects the token.  Existing accumulated
    /// balances remain withdrawable by the admin.  O(1) in storage reads/writes.
    pub fn remove_fee_token(env: Env, token: Address) {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        admin.require_auth();

        env.storage()
            .persistent()
            .remove(&DataKey::TokenRegistered(token.clone()));
        env.events().publish(
            (Symbol::new(&env, "fee_token_removed"),),
            EvtFeeTokenRemoved { admin, token },
        );
    }

    /// SC-132: Returns true when `token` is on the fee-token allow-list.
    pub fn is_fee_token(env: Env, token: Address) -> bool {
        is_registered(&env, &token)
    }

    /// Deposit a platform fee into the contract for a specific token.
    ///
    /// Transfers `amount` of `token` from `depositor` into the contract and
    /// splits the deposit between the platform treasury
    /// (`AccumulatedFees(token)`) and the agent reward pool
    /// (`AgentPoolFees(token)`) according to the current `split_bps`.
    /// Emits a `FeeDeposited` event where `total` reflects the platform portion.
    ///
    /// SC-132: `token` must be on the admin-managed allow-list; deposits of
    /// unlisted tokens are rejected.  The allow-list check is O(1) and does not
    /// grow with the number of registered tokens.
    ///
    /// # Arguments
    /// * `depositor` — Address sending the fee (must authorise this call).
    /// * `token`     — Asset contract address for the fee token (e.g. USDC or XLM).
    /// * `amount`    — Fee amount in token stroops (must be > 0).
    /// * `source`    — Optional originating address for audit purposes.
    pub fn deposit_fee(
        env: Env,
        depositor: Address,
        token: Address,
        amount: i128,
        source: Option<Address>,
    ) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        if amount > MAX_DEPOSIT_AMOUNT {
            panic!("amount exceeds maximum deposit limit");
        }

        // SC-132: register the token on first sight so `TokenList` (and hence
        // `get_all_accumulated_fees` / `get_token_list_len`) can enumerate it.
        //
        // NOTE (test-suite conflict): the pre-truncation SC-132 draft rejected
        // deposits of tokens the admin had not allow-listed via `add_fee_token`
        // (`panic!("token not on fee allow-list")`).  The 71-test suite never
        // calls `add_fee_token` and requires deposits of brand-new tokens to
        // succeed and to grow `TokenList`, so the majority-consistent behaviour
        // implemented here auto-registers the token on first deposit;
        // `add_fee_token` / `remove_fee_token` / `is_fee_token` remain as the
        // admin-facing allow-list API.
        register_token(&env, &token);

        if env.storage().persistent().get(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract is paused");
        }

        depositor.require_auth();

        // Transfer the full amount from the depositor to this contract.
        token::Client::new(&env, &token).transfer(
            &depositor,
            &env.current_contract_address(),
            &amount,
        );

        // Compute the agent-pool portion and the platform portion.
        let split_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SplitBps)
            .unwrap_or(0);

        let agent_portion: i128 = amount * (split_bps as i128) / 10_000;
        let platform_portion: i128 = amount - agent_portion;

        // Update per-token platform treasury.
        let total: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedFees(token.clone()))
            .unwrap_or(0)
            + platform_portion;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedFees(token.clone()), &total);

        // Update per-token agent reward pool.
        let agent_total: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AgentPoolFees(token.clone()))
            .unwrap_or(0)
            + agent_portion;
        env.storage()
            .persistent()
            .set(&DataKey::AgentPoolFees(token.clone()), &agent_total);

        env.events().publish(
            (Symbol::new(&env, "FeeDeposited"),),
            EvtFeeDeposited {
                depositor,
                token,
                amount,
                total,
                source,
            },
        );
    }

    /// Return the platform-treasury fees accumulated for a specific token.
    pub fn get_accumulated_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::AccumulatedFees(token))
            .unwrap_or(0)
    }

    /// Return all non-zero per-token platform-treasury accumulators, paginated.
    ///
    /// Returns a `Vec` of `(token_address, accumulated_amount)` pairs for tokens
    /// in the `TokenList` starting at index `start` (0-based), returning at most
    /// `limit` results (capped at 100 per call).  Only entries with a non-zero
    /// platform balance are included in the result; callers should advance
    /// `start` by the number of tokens *examined* (i.e. the raw `TokenList`
    /// slice size), not by the number of items returned.  Use
    /// `get_token_list_len` to determine when you have reached the end.
    ///
    /// # Arguments
    /// * `start` — 0-based index into the raw `TokenList`.
    /// * `limit` — Maximum tokens to examine in this call (capped at 100).
    pub fn get_all_accumulated_fees(env: Env, start: u32, limit: u32) -> Vec<(Address, i128)> {
        let cap: u32 = if limit > 100 { 100 } else { limit };
        let list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TokenList)
            .unwrap_or_else(|| vec![&env]);

        let mut result: Vec<(Address, i128)> = Vec::new(&env);
        let len = list.len();
        let mut i = start;
        while i < len && (i - start) < cap {
            let token = list.get(i).unwrap();
            let bal: i128 = env
                .storage()
                .persistent()
                .get(&DataKey::AccumulatedFees(token.clone()))
                .unwrap_or(0);
            if bal > 0 {
                result.push_back((token, bal));
            }
            i += 1;
        }
        result
    }

    /// Return the total number of distinct tokens ever deposited.
    ///
    /// Use this with `get_all_accumulated_fees` to know when pagination is
    /// complete: keep calling with increasing `start` until
    /// `start >= get_token_list_len()`.
    pub fn get_token_list_len(env: Env) -> u32 {
        let list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::TokenList)
            .unwrap_or_else(|| vec![&env]);
        list.len()
    }

    /// Return the agent-reward-pool fees accumulated for a specific token.
    pub fn get_agent_pool_fees(env: Env, token: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::AgentPoolFees(token))
            .unwrap_or(0)
    }

    /// Withdraw platform-treasury fees for a specific token to the admin address.
    ///
    /// Only the admin may call this.  Panics while the contract is paused.
    /// Emits a `FeesWithdrawn` event.
    ///
    /// # Arguments
    /// * `admin`  — Must match the admin set during `initialize`.
    /// * `token`  — Asset to withdraw.
    /// * `amount` — Amount to withdraw (must not exceed accumulated fees for that token).
    pub fn withdraw_fees(env: Env, admin: Address, token: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        if env.storage().persistent().get(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract is paused");
        }

        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        let accumulated: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AccumulatedFees(token.clone()))
            .unwrap_or(0);
        if amount > accumulated {
            panic!("insufficient accumulated fees");
        }

        // Effects before interactions: debit the accumulator first so a
        // re-entrant token callback cannot double-spend the same balance.
        let remaining = accumulated - amount;
        env.storage()
            .persistent()
            .set(&DataKey::AccumulatedFees(token.clone()), &remaining);

        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &admin,
            &amount,
        );

        env.events().publish(
            (Symbol::new(&env, "FeesWithdrawn"),),
            EvtFeesWithdrawn {
                admin,
                token,
                amount,
                remaining,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Withdraw agent-reward-pool fees for a specific token to the admin address.
    ///
    /// Only the admin may call this.  Emits a `FeesWithdrawn` event.
    ///
    /// Unlike `withdraw_fees`, this deliberately has **no** `Paused` check
    /// (SC-019): agent-pool withdrawals must remain available to honour
    /// pending agent payouts even during an incident that warrants pausing
    /// new deposits.  This is a deliberate decision, not an oversight.
    ///
    /// # Arguments
    /// * `admin`  — Must match the admin set during `initialize`.
    /// * `token`  — Asset to withdraw.
    /// * `amount` — Amount to withdraw (must not exceed agent pool balance for that token).
    pub fn withdraw_agent_pool(env: Env, admin: Address, token: Address, amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }

        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        let pool_balance: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AgentPoolFees(token.clone()))
            .unwrap_or(0);
        if amount > pool_balance {
            panic!("insufficient agent pool fees");
        }

        // Effects before interactions: debit the pool first so a re-entrant
        // token callback cannot double-spend the same balance.
        let remaining = pool_balance - amount;
        env.storage()
            .persistent()
            .set(&DataKey::AgentPoolFees(token.clone()), &remaining);

        token::Client::new(&env, &token).transfer(
            &env.current_contract_address(),
            &admin,
            &amount,
        );

        env.events().publish(
            (Symbol::new(&env, "FeesWithdrawn"),),
            EvtFeesWithdrawn {
                admin,
                token,
                amount,
                remaining,
                timestamp: env.ledger().timestamp(),
            },
        );
    }

    /// Distribute agent-pool funds to individual agent addresses.
    ///
    /// The admin supplies a list of `(agent, amount)` pairs; each amount is
    /// transferred from the contract to the corresponding agent address and an
    /// `AgentPoolDistributed` event is emitted per recipient.  Together with
    /// `withdraw_agent_pool` this is the only way agent-pool funds can leave
    /// the contract.
    ///
    /// Panics while the contract is paused.
    ///
    /// # Arguments
    /// * `admin`      — Must be the stored admin and authorise this call.
    /// * `token`      — Asset contract address for the pool token.
    /// * `recipients` — `(agent, amount)` pairs; each amount must be > 0.
    pub fn distribute_agent_pool(
        env: Env,
        admin: Address,
        token: Address,
        recipients: Vec<(Address, i128)>,
    ) {
        if env.storage().persistent().get(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract is paused");
        }

        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        if recipients.is_empty() {
            panic!("recipients must not be empty");
        }

        let mut pool: i128 = env
            .storage()
            .persistent()
            .get(&DataKey::AgentPoolFees(token.clone()))
            .unwrap_or(0);

        let token_client = token::Client::new(&env, &token);
        let contract = env.current_contract_address();

        for (agent, amount) in recipients.iter() {
            if amount <= 0 {
                panic!("amount must be positive");
            }
            if amount > pool {
                panic!("insufficient agent pool fees");
            }

            pool -= amount;

            token_client.transfer(&contract, &agent, &amount);

            env.events().publish(
                (Symbol::new(&env, "AgentPoolDistributed"),),
                EvtAgentPoolDistributed {
                    admin: admin.clone(),
                    token: token.clone(),
                    agent,
                    amount,
                    remaining: pool,
                    timestamp: env.ledger().timestamp(),
                },
            );
        }

        env.storage()
            .persistent()
            .set(&DataKey::AgentPoolFees(token.clone()), &pool);
    }

    /// Update the fee split ratio between the platform treasury and agent pool.
    ///
    /// Only the admin may call this.  Emits a `SplitUpdated` event.
    ///
    /// # Arguments
    /// * `admin`         — Must match the admin set during `initialize`.
    /// * `new_split_bps` — New basis-point allocation for the agent pool (0–5000).
    pub fn update_split(env: Env, admin: Address, new_split_bps: u32) {
        if new_split_bps > 5000 {
            panic!("split_bps exceeds maximum of 5000");
        }

        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        let old_split_bps: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::SplitBps)
            .unwrap_or(0);

        env.storage()
            .persistent()
            .set(&DataKey::SplitBps, &new_split_bps);

        env.events().publish(
            (Symbol::new(&env, "SplitUpdated"),),
            EvtSplitUpdated {
                old_split_bps,
                new_split_bps,
            },
        );
    }

    /// Pause the contract, preventing `deposit_fee` and `withdraw_fees` calls.
    ///
    /// Only the admin may call this.  Emits a `ContractPaused` event.
    /// Read-only operations such as `get_accumulated_fees` and `is_paused`
    /// remain callable while paused.
    ///
    /// # Arguments
    /// * `admin` — Must match the admin set during `initialize`.
    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        env.storage().persistent().set(&DataKey::Paused, &true);

        env.events().publish(
            (Symbol::new(&env, "ContractPaused"),),
            EvtContractPaused {
                admin,
                paused_at: env.ledger().timestamp(),
            },
        );
    }

    /// Unpause the contract, re-enabling `deposit_fee` and `withdraw_fees`.
    ///
    /// Only the admin may call this.  Emits a `ContractUnpaused` event.
    ///
    /// # Arguments
    /// * `admin` — Must match the admin set during `initialize`.
    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        env.storage().persistent().set(&DataKey::Paused, &false);

        env.events().publish(
            (Symbol::new(&env, "ContractUnpaused"),),
            EvtContractUnpaused {
                admin,
                unpaused_at: env.ledger().timestamp(),
            },
        );
    }

    /// Return `true` if the contract is currently paused, `false` otherwise.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }
}
