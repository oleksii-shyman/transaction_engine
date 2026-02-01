use std::io::Write;
use std::process::Command;
use tempfile::NamedTempFile;

fn run_with_csv(csv: &str) -> String {
    let bin = env!("CARGO_BIN_EXE_transaction_processing");

    let mut tmp = NamedTempFile::new().expect("create temp csv");
    tmp.write_all(csv.as_bytes()).expect("write csv");
    let path = tmp.into_temp_path();

    let output = Command::new(bin)
        .arg(&path)
        .output()
        .expect("failed to execute binary");

    assert!(
        output.status.success(),
        "process failed: status={:?} stderr={}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout not utf8");

    // keep the temp file alive until after assertions
    let _ = path.close();
    stdout
}

#[test]
fn runs_sample_input_csv() {
    let csv = "\
type,client,tx,amount
deposit,1,1,1.0
deposit,2,2,2.0
deposit,1,3,2.0
withdrawal,1,4,1.5
withdrawal,2,5,3.0
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,1.5,0,1.5,false
2,2.0,0,2.0,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn resolve_restores_funds() {
    let csv = "\
type,client,tx,amount
deposit,1,1,100
dispute,1,1,
resolve,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,100,0,100,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn chargeback_locks_and_blocks_new_tx() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
dispute,1,1,
chargeback,1,1,
deposit,1,2,5
withdrawal,1,3,1
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,0,0,0,true
";
    assert_eq!(stdout, expected);
}

#[test]
fn dispute_left_unresolved_keeps_funds_held() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
withdrawal,1,2,3
dispute,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,-3,10,7,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn duplicate_transaction_ids_are_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
withdrawal,1,1,5
deposit,2,1,3
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,10,0,10,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn client_id_mismatch_dispute_is_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
dispute,2,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,10,0,10,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn repeat_disputes_after_first_are_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,5
dispute,1,1,
dispute,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,0,5,5,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn resolve_without_dispute_is_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,5
resolve,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,5,0,5,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn chargeback_without_dispute_is_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,5
chargeback,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,5,0,5,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn unknown_transaction_commands_are_ignored() {
    let csv = "\
type,client,tx,amount
dispute,1,42,
resolve,1,42,
chargeback,1,42,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
";
    assert_eq!(stdout, expected);
}

#[test]
fn wrong_client_resolve_or_chargeback_is_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,5
dispute,1,1,
resolve,2,1,
chargeback,2,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,0,5,5,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn locked_account_blocks_all_followups() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
dispute,1,1,
chargeback,1,1,
deposit,1,2,5
withdrawal,1,3,1
dispute,1,2,
resolve,1,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,0,0,0,true
";
    assert_eq!(stdout, expected);
}

#[test]
fn dispute_on_withdrawal_is_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
withdrawal,1,2,3
dispute,2,1,
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,7,0,7,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn locked_client_does_not_block_others() {
    let csv = "\
type,client,tx,amount
deposit,1,1,10
dispute,1,1,
chargeback,1,1,
deposit,2,2,5
withdrawal,2,3,2
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,0,0,0,true
2,3,0,3,false
";
    assert_eq!(stdout, expected);
}

#[test]
fn malformed_rows_are_ignored() {
    let csv = "\
type,client,tx,amount
deposit,1,1,5
deposit,1,2,   \n\
withdrawal,1,3,1.5
";
    let stdout = run_with_csv(csv);
    let expected = "\
client,available,held,total,locked
1,3.5,0,3.5,false
";
    assert_eq!(stdout, expected);
}
