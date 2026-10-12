#![cfg(test)]

//! Unit + workflow ("integration") tests for the savings-circle contract.
//!
//! Conventions follow the sibling crates: `setup()` helper, SAC token via
//! `register_stellar_asset_contract` (20.x API), `mock_all_auths`,
//! ledger-time manipulation. A non-zero base timestamp is set because the
//! suite is sensitive to `!= 0` sentinels and realistic ledger times.

use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::{StellarAssetClient, TokenClient},
    Address, Env, String, Vec,
};

use crate::{CircleStatus, SavingsCircleContract, SavingsCircleContractClient};

const CYCLE: u64 = 3_600;
const CONTRIBUTION: i128 = 100;
const COLLATERAL: i128 = 50;
const JOIN_COST: i128 = CONTRIBUTION + COLLATERAL;
const FUND: i128 = 10_000;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn setup_with_fee(fee_bps: u32) -> (Env, SavingsCircleContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 100);
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsCircleContract);
    let client = SavingsCircleContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.initialize(&admin, &token, &fee_bps);
    (env, client, admin, token)
}

fn setup() -> (Env, SavingsCircleContractClient<'static>, Address, Address) {
    setup_with_fee(0)
}

fn name(env: &Env) -> String {
    String::from_str(env, "Test Circle")
}

fn mint(env: &Env, token: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token).mint(to, &amount);
}

fn approve(env: &Env, token: &Address, from: &Address, spender: &Address, amount: i128) {
    // SDK 20.x token interface: approve with a far-future expiration stands in
    // for increase_allowance (added in SDK 21).
    TokenClient::new(env, token).approve(from, spender, &amount, &6_000_000); // < host max live_until
}

fn balance(env: &Env, token: &Address, who: &Address) -> i128 {
    TokenClient::new(env, token).balance(who)
}

/// Fund `member` and approve the circle contract to pull up to `amount`.
fn fund_and_approve(
    env: &Env,
    token: &Address,
    contract: &Address,
    member: &Address,
    amount: i128,
) {
    mint(env, token, member, amount);
    approve(env, token, member, contract, amount);
}

fn advance(env: &Env, secs: u64) {
    env.ledger().with_mut(|li| li.timestamp += secs);
}

/// Create an Open circle (3 members max, no fee) and return its id.
fn open_circle(client: &SavingsCircleContractClient, creator: &Address) -> u64 {
    client.create_circle(
        creator,
        &name(&client.env),
        &CONTRIBUTION,
        &CYCLE,
        &3,
        &COLLATERAL)
}

/// Open + join three funded members + start. Returns (circle_id, members).
fn active_circle(
    env: &Env,
    client: &SavingsCircleContractClient,
    token: &Address,
    creator: &Address,
) -> (u64, Vec<Address>) {
    let contract = client.registered_address();
    let id = open_circle(client, creator);
    let m0 = Address::generate(env);
    let m1 = Address::generate(env);
    let m2 = Address::generate(env);
    for m in [&m0, &m1, &m2] {
        fund_and_approve(env, token, &contract, m, FUND);
        client.join_circle(m, &id);
    }
    client.start_circle(creator, &id);
    (id, Vec::from_array(&env, [m0, m1, m2]))
}

trait ClientExt {
    fn registered_address(&self) -> Address;
}

impl<'a> ClientExt for SavingsCircleContractClient<'a> {
    fn registered_address(&self) -> Address {
        self.address.clone()
    }
}

// ── initialize ────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_works() {
    let (env, client, _admin, token) = setup();
    assert_eq!(client.get_token(), token);
    assert_eq!(client.get_fees(), 0);
    // Silence unused warning; env kept for symmetry with other suites.
    let _ = env;
}

#[test]
#[should_panic(expected = "Error(Contract, #1)")]
fn test_double_initialize_panics() {
    let (_, client, admin, token) = setup();
    client.initialize(&admin, &token, &0);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_initialize_fee_bps_over_max_panics() {
    let env = Env::default();
    env.ledger().with_mut(|li| li.timestamp = 100);
    env.mock_all_auths();
    let id = env.register_contract(None, SavingsCircleContract);
    let client = SavingsCircleContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.initialize(&admin, &token, &1_001);
}

// ── create_circle ─────────────────────────────────────────────────────────────

#[test]
fn test_create_circle_happy() {
    let (env, client, admin, _token) = setup();
    let id = open_circle(&client, &admin);
    assert_eq!(id, 1);
    let circle = client.get_circle(&id);
    assert_eq!(circle.status, CircleStatus::Open);
    assert_eq!(circle.contribution, CONTRIBUTION);
    assert_eq!(circle.collateral, COLLATERAL);
    assert_eq!(circle.cycle_seconds, CYCLE);
    assert_eq!(circle.max_members, 3);
    assert_eq!(circle.active_count, 0);
    assert_eq!(circle.pot, 0);
    assert_eq!(client.get_members(&id, &0, &10).len(), 0);
    let _ = env;
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_zero_contribution_panics() {
    let (_, client, admin, _) = setup();
    client.create_circle(&admin, &name(&client.env), &0, &CYCLE, &3, &COLLATERAL);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_too_short_cycle_panics() {
    let (_, client, admin, _) = setup();
    client.create_circle(&admin, &name(&client.env), &CONTRIBUTION, &59, &3, &COLLATERAL);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_too_few_members_panics() {
    let (_, client, admin, _) = setup();
    client.create_circle(&admin, &name(&client.env), &CONTRIBUTION, &CYCLE, &1, &COLLATERAL);
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_negative_collateral_panics() {
    let (_, client, admin, _) = setup();
    client.create_circle(&admin, &name(&client.env), &CONTRIBUTION, &CYCLE, &3, &(-1));
}

#[test]
#[should_panic(expected = "Error(Contract, #3)")]
fn test_create_empty_name_panics() {
    let (_, client, admin, _) = setup();
    let empty = String::from_str(&client.env, "");
    client.create_circle(&admin, &empty, &CONTRIBUTION, &CYCLE, &3, &COLLATERAL);
}

// ── join_circle ───────────────────────────────────────────────────────────────

#[test]
fn test_join_circle_pulls_funds_and_tracks_member() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);

    client.join_circle(&m0, &id);

    assert_eq!(balance(&env, &token, &m0), FUND - JOIN_COST);
    let circle = client.get_circle(&id);
    assert_eq!(circle.active_count, 1);
    assert_eq!(circle.pot, CONTRIBUTION);
    let meta = client.get_member(&id, &m0);
    assert_eq!(meta.slot, 0);
    assert!(meta.active);
    assert_eq!(meta.total_contributed, CONTRIBUTION);
    assert_eq!(client.get_members(&id, &0, &10).len(), 1);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_join_twice_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    client.join_circle(&m0, &id);
    client.join_circle(&m0, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_join_after_start_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    fund_and_approve(&env, &token, &contract, &m1, FUND);
    client.join_circle(&m0, &id);
    client.join_circle(&m1, &id);
    client.start_circle(&admin, &id);
    client.join_circle(&m1, &id); // m1 already joined — but status check fires first
}

#[test]
#[should_panic(expected = "Error(Contract, #9)")]
fn test_join_full_circle_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = client.create_circle(
        &admin,
        &name(&client.env),
        &CONTRIBUTION,
        &CYCLE,
        &2,
        &COLLATERAL);
    for _ in 0..2 {
        let m = Address::generate(&env);
        fund_and_approve(&env, &token, &contract, &m, FUND);
        client.join_circle(&m, &id);
    }
    let m3 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m3, FUND);
    client.join_circle(&m3, &id);
}

// ── start_circle ──────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #10)")]
fn test_start_needs_two_members_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    client.join_circle(&m0, &id);
    client.start_circle(&admin, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_start_by_stranger_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    fund_and_approve(&env, &token, &contract, &m1, FUND);
    client.join_circle(&m0, &id);
    client.join_circle(&m1, &id);
    let stranger = Address::generate(&env);
    client.start_circle(&stranger, &id);
}

#[test]
fn test_start_circle_happy() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    for _ in 0..2 {
        let m = Address::generate(&env);
        fund_and_approve(&env, &token, &contract, &m, FUND);
        client.join_circle(&m, &id);
    }
    let before = client.env.ledger().timestamp();
    client.start_circle(&admin, &id);
    let circle = client.get_circle(&id);
    assert_eq!(circle.status, CircleStatus::Active);
    assert_eq!(circle.cycle_deadline, before + CYCLE);
    assert_eq!(circle.current_cycle, 0);
}

// ── contribute ────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_contribute_before_start_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    client.join_circle(&m0, &id);
    client.contribute(&m0, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_contribute_cycle0_after_join_panics() {
    // Join already credited the member's cycle-0 contribution.
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    client.contribute(&m0, &id);
}

#[test]
fn test_contribute_later_cycle_works() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let before = balance(&env, &token, &m0);

    advance(&env, CYCLE);
    client.rollover(&admin, &id); // cycle 0 → 1
    client.contribute(&m0, &id);

    // m0 receives the cycle-0 pot (3 * CONTRIBUTION) then pays their
    // cycle-1 contribution in the same rollover window.
    assert_eq!(balance(&env, &token, &m0), before + 2 * CONTRIBUTION);
    let meta = client.get_member(&id, &m0);
    assert_eq!(meta.total_contributed, 2 * CONTRIBUTION);
    assert_eq!(client.get_circle(&id).pot, CONTRIBUTION);
}

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_contribute_twice_in_cycle_panics() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    client.contribute(&m0, &id);
    client.contribute(&m0, &id);
}

// ── rollover ──────────────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #11)")]
fn test_rollover_too_early_panics() {
    let (env, client, admin, token) = setup();
    let (id, _members) = active_circle(&env, &client, &token, &admin);
    advance(&env, CYCLE - 1);
    client.rollover(&admin, &id);
}

#[test]
fn test_rollover_pays_first_member() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let before = balance(&env, &token, &m0);

    advance(&env, CYCLE);
    client.rollover(&admin, &id);

    // pot = 3 × CONTRIBUTION, no fee → m0 (slot 0, cycle 0) receives it all.
    assert_eq!(balance(&env, &token, &m0), before + 3 * CONTRIBUTION);
    let circle = client.get_circle(&id);
    assert_eq!(circle.current_cycle, 1);
    assert_eq!(circle.pot, 0);
    assert_eq!(circle.cycle_deadline, 100 + CYCLE + CYCLE);
    let meta = client.get_member(&id, &m0);
    assert_eq!(meta.total_received, 3 * CONTRIBUTION);
    assert_eq!(client.get_fees(), 0);
}

#[test]
fn test_rollover_rotation_pays_second_member_next() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();

    advance(&env, CYCLE);
    client.rollover(&admin, &id); // pays m0
    advance(&env, CYCLE);
    // Everyone contributes to cycle 1.
    for m in members.iter() {
        client.contribute(&m, &id);
    }
    client.rollover(&admin, &id); // pays m1

    let m2 = members.get(2).unwrap();
    let pot1 = 3 * CONTRIBUTION;
    assert_eq!(client.get_member(&id, &m0).total_received, pot1);
    assert_eq!(client.get_member(&id, &m1).total_received, pot1);
    assert_eq!(client.get_member(&id, &m2).total_received, 0);
}

#[test]
fn test_rollover_drops_defector_and_forfeits_collateral() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();
    let m2 = members.get(2).unwrap();

    advance(&env, CYCLE);
    client.rollover(&admin, &id); // cycle 0 → 1, pays m0

    // Cycle 1: m2 defects (no contribution).
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.contribute(&m1, &id);
    let m2_before = balance(&env, &token, &m2);
    client.rollover(&admin, &id);

    // m2 dropped; collateral forfeited into pot; active = [m0, m1];
    // recipient index = cycle 1 % 2 = 1 → m1.
    assert!(!client.get_member(&id, &m2).active);
    assert_eq!(balance(&env, &token, &m2), m2_before); // no refund
    let expected_pot = 2 * CONTRIBUTION + COLLATERAL;
    assert_eq!(client.get_member(&id, &m1).total_received, expected_pot);
    // m0 received the cycle-0 payout.
    assert_eq!(client.get_member(&id, &m0).total_received, 3 * CONTRIBUTION);
    // 2 active remain → rotation still possible, circle stays Active.
    assert_eq!(client.get_circle(&id).status, CircleStatus::Active);

    // Cycle 2: m1 defects too → only m0 left → Completed.
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.rollover(&admin, &id);
    assert_eq!(client.get_circle(&id).status, CircleStatus::Completed);
}

#[test]
fn test_rollover_all_defect_completes_circle() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);

    advance(&env, CYCLE);
    client.rollover(&admin, &id); // cycle 0 paid normally

    // Cycle 1: nobody contributes.
    advance(&env, CYCLE);
    client.rollover(&admin, &id);

    let circle = client.get_circle(&id);
    assert_eq!(circle.status, CircleStatus::Completed);
    for m in members.iter() {
        assert!(!client.get_member(&id, &m).active);
    }
    // No extra tokens moved: only the cycle-0 payout left the contract.
    assert_eq!(
        balance(&env, &token, &client.registered_address()),
        3 * COLLATERAL // all three members' collateral still held
    );
}

#[test]
fn test_rollover_emits_events() {
    let (env, client, admin, token) = setup();
    let (id, _members) = active_circle(&env, &client, &token, &admin);
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    let events = env.events().all();
    // At least: 3×joined, 1×started, 1×rollover.
    assert!(events.len() >= 5);
}

// ── fees ──────────────────────────────────────────────────────────────────────

#[test]
fn test_rollover_accrues_fee_and_admin_withdraws() {
    let (env, client, admin, token) = setup_with_fee(100); // 1%
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let before = balance(&env, &token, &m0);

    advance(&env, CYCLE);
    client.rollover(&admin, &id);

    let pot = 3 * CONTRIBUTION; // 300
    let fee = pot * 100 / 10_000; // 3
    assert_eq!(balance(&env, &token, &m0), before + pot - fee);
    assert_eq!(client.get_fees(), fee);

    let treasury = Address::generate(&env);
    client.withdraw_fees(&admin, &treasury);
    assert_eq!(balance(&env, &token, &treasury), fee);
    assert_eq!(client.get_fees(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_withdraw_fees_non_admin_panics() {
    let (env, client, admin, token) = setup_with_fee(100);
    let (id, _members) = active_circle(&env, &client, &token, &admin);
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    let impostor = Address::generate(&env);
    client.withdraw_fees(&impostor, &impostor);
}

#[test]
#[should_panic(expected = "Error(Contract, #12)")]
fn test_withdraw_fees_empty_panics() {
    let (_, client, admin, _) = setup();
    let treasury = Address::generate(&client.env);
    client.withdraw_fees(&admin, &treasury);
}

// ── claim_collateral ──────────────────────────────────────────────────────────

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_claim_collateral_before_completion_panics() {
    let (env, client, admin, token) = setup();
    let (id, _members) = active_circle(&env, &client, &token, &admin);
    let circle = client.get_circle(&id);
    let _ = circle;
    // Still Active.
    let member = client.get_members(&id, &0, &1).get(0).unwrap();
    client.claim_collateral(&member, &id);
}

#[test]
fn test_claim_collateral_after_completion() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);

    // Cycle 0: normal payout. Cycle 1: all defect → completed.
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    assert_eq!(client.get_circle(&id).status, CircleStatus::Completed);

    // Everyone was dropped → nobody may claim (collateral was forfeited).
    for m in members.iter() {
        assert!(!client.get_member(&id, &m).active);
    }
}

#[test]
fn test_claim_collateral_returns_funds_to_survivors() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();
    let m2 = members.get(2).unwrap();

    // Cycle 0: all pay; m0 receives. Cycle 1: only m0 pays → m1, m2 dropped;
    // recipient = active[1 % 1] = m0 again.
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.rollover(&admin, &id);
    assert_eq!(client.get_circle(&id).status, CircleStatus::Completed);

    let before = balance(&env, &token, &m0);
    client.claim_collateral(&m0, &id);
    assert_eq!(balance(&env, &token, &m0), before + COLLATERAL);

    // Double claim rejected.
    // (dropped members m1/m2 cannot claim — active == false)
    assert!(!client.get_member(&id, &m1).active);
    assert!(!client.get_member(&id, &m2).active);
}

#[test]
#[should_panic(expected = "Error(Contract, #13)")]
fn test_claim_collateral_twice_panics() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.rollover(&admin, &id);
    client.claim_collateral(&m0, &id);
    client.claim_collateral(&m0, &id);
}

// ── cancel ────────────────────────────────────────────────────────────────────

#[test]
fn test_cancel_refunds_everyone() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    let m1 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    fund_and_approve(&env, &token, &contract, &m1, FUND);
    client.join_circle(&m0, &id);
    client.join_circle(&m1, &id);

    client.cancel_circle(&admin, &id);

    assert_eq!(balance(&env, &token, &m0), FUND);
    assert_eq!(balance(&env, &token, &m1), FUND);
    assert_eq!(client.get_circle(&id).status, CircleStatus::Cancelled);
    assert_eq!(balance(&env, &token, &contract), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_cancel_after_start_panics() {
    let (env, client, admin, token) = setup();
    let (id, _members) = active_circle(&env, &client, &token, &admin);
    client.cancel_circle(&admin, &id);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_cancel_by_non_creator_panics() {
    let (env, client, admin, token) = setup();
    let contract = client.registered_address();
    let id = open_circle(&client, &admin);
    let m0 = Address::generate(&env);
    fund_and_approve(&env, &token, &contract, &m0, FUND);
    client.join_circle(&m0, &id);
    client.cancel_circle(&m0, &id);
}

// ── pagination & lookups ──────────────────────────────────────────────────────

#[test]
fn test_get_members_pagination() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let page = client.get_members(&id, &1, &1);
    assert_eq!(page.len(), 1);
    assert_eq!(page.get(0).unwrap(), members.get(1).unwrap());
    assert_eq!(client.get_members(&id, &3, &10).len(), 0);
    let capped = client.get_members(&id, &0, &9_999);
    assert_eq!(capped.len(), 3); // limit capped, not an error
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_circle_not_found_panics() {
    let (_, client, _, _) = setup();
    client.get_circle(&42);
}

#[test]
fn test_get_counter() {
    let (_, client, admin, _) = setup();
    assert_eq!(client.get_counter(), 0);
    open_circle(&client, &admin);
    assert_eq!(client.get_counter(), 1);
    open_circle(&client, &admin);
    assert_eq!(client.get_counter(), 2);
}

// ── invariants ────────────────────────────────────────────────────────────────

#[test]
fn test_invariant_conservation_no_fee() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m0 = members.get(0).unwrap();
    let m1 = members.get(1).unwrap();
    let m2 = members.get(2).unwrap();

    // Cycle 0: all joined (credited at join) → m0 receives 3×C.
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    // Cycle 1: m2 defects → forfeits collateral; m1 receives 2×C + collateral.
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.contribute(&m1, &id);
    client.rollover(&admin, &id);
    // Cycle 2: m1 defects; m0 receives C + collateral → Completed.
    advance(&env, CYCLE);
    client.contribute(&m0, &id);
    client.rollover(&admin, &id);
    assert_eq!(client.get_circle(&id).status, CircleStatus::Completed);

    let mut contributed = 0i128;
    let mut received = 0i128;
    for m in members.iter() {
        let meta = client.get_member(&id, &m);
        contributed += meta.total_contributed;
        received += meta.total_received;
    }
    // Forfeited collaterals (m1 + m2) bridge contributed → received.
    assert_eq!(contributed + 2 * COLLATERAL, received);

    // m0 (the only member never dropped) claims collateral → drains to zero.
    client.claim_collateral(&m0, &id);
    assert_eq!(balance(&env, &token, &client.registered_address()), 0);
}

#[test]
fn test_invariant_no_transfer_to_dropped_member() {
    let (env, client, admin, token) = setup();
    let (id, members) = active_circle(&env, &client, &token, &admin);
    let m2 = members.get(2).unwrap();

    // m2 drops in cycle 1; run cycle 2 as well to be sure nothing routes to them.
    advance(&env, CYCLE);
    client.rollover(&admin, &id);
    advance(&env, CYCLE);
    client.contribute(&members.get(0).unwrap(), &id);
    client.contribute(&members.get(1).unwrap(), &id);
    client.rollover(&admin, &id); // 2 active remain → circle stays Active

    assert_eq!(client.get_member(&id, &m2).total_received, 0);
    assert!(!client.get_member(&id, &m2).active);
}
