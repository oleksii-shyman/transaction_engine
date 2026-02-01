use super::*;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

fn mk_row(typ: &str, client: ClientID, tx: TransactionID, amount: Option<&str>) -> InputRow {
    InputRow {
        transaction_type: typ.to_string(),
        client_id: client,
        transaction_id: tx,
        amount: amount.map(|s| s.to_string()),
    }
}

#[test]
fn deposit_then_withdraw_updates_balances() {
    let mut engine = Engine::default();
    engine.apply(mk_row("deposit", 1, 1, Some("10")));
    engine.apply(mk_row("withdrawal", 1, 2, Some("4")));

    let acc = &engine.accounts[&1];
    assert_eq!(acc.available, dec!(6));
    assert_eq!(acc.held, dec!(0));
    assert_eq!(acc.total(), dec!(6));
}

#[test]
fn negative_amount_is_rejected() {
    let mut engine = Engine::default();
    engine.apply(mk_row("deposit", 1, 1, Some("-1")));
    assert!(!engine.accounts.contains_key(&1));
    assert!(!engine.transactions.contains_key(&1));

    // also ensure parser alone errors
    assert!(parse_amount("-1").is_err());
}

#[test]
fn disputes_apply_only_to_deposits() {
    let mut engine = Engine::default();
    engine.apply(mk_row("deposit", 1, 1, Some("5")));
    engine.apply(mk_row("withdrawal", 1, 2, Some("2")));

    // disputing a withdrawal should be ignored
    engine.apply(mk_row("dispute", 1, 2, None));
    let acc = &engine.accounts[&1];
    assert_eq!(acc.available, dec!(3));
    assert_eq!(acc.held, dec!(0));

    // disputing the deposit should move funds to held
    engine.apply(mk_row("dispute", 1, 1, None));
    let acc = &engine.accounts[&1];
    assert_eq!(acc.available, dec!(-2));
    assert_eq!(acc.held, dec!(5));
}

#[test]
fn withdrawal_more_than_available_is_ignored() {
    let mut engine = Engine::default();
    engine.apply(mk_row("deposit", 1, 1, Some("5")));
    engine.apply(mk_row("withdrawal", 1, 2, Some("10")));

    let acc = &engine.accounts[&1];
    assert_eq!(acc.available, dec!(5));
    assert_eq!(acc.total(), dec!(5));
    assert!(!engine.transactions.contains_key(&2));
}

#[test]
fn parse_amount_rejects_zero_and_too_many_decimals() {
    assert!(parse_amount("0").is_err());
    assert!(parse_amount("1.23456").is_err());
    // boundary: exactly 4 dp passes unchanged
    assert_eq!(parse_amount("1.2345").unwrap(), dec!(1.2345));
}

#[test]
fn dispute_can_make_available_negative_per_spec() {
    let mut engine = Engine::default();
    engine.apply(mk_row("deposit", 1, 1, Some("100")));
    engine.apply(mk_row("withdrawal", 1, 2, Some("100")));

    engine.apply(mk_row("dispute", 1, 1, None));
    let acc = &engine.accounts[&1];
    assert_eq!(acc.available, dec!(-100));
    assert_eq!(acc.held, dec!(100));
    assert_eq!(acc.total(), dec!(0));
}

#[test]
fn decimal_output_keeps_four_places() {
    let d = Decimal::new(15, 1); // 1.5
    assert_eq!(d.round_dp(4).to_string(), "1.5");

    let d = Decimal::new(12222222, 7); // 1.2222
    assert_eq!(d.round_dp(4).to_string(), "1.2222");

    let d = Decimal::new(2, 0); // 2
    assert_eq!(d.round_dp(4).to_string(), "2");
}
