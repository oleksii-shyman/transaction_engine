use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io;

type ClientID = u64;
type TransactionID = u64;
type Currency = Decimal;

#[derive(Debug, Deserialize)]
struct InputRow {
    #[serde(rename = "type")]
    transaction_type: String,
    #[serde(rename = "client")]
    client_id: ClientID,
    #[serde(rename = "tx")]
    transaction_id: TransactionID,
    #[serde(default)]
    amount: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct Account {
    available: Currency,
    held: Currency,
    locked: bool,
}

impl Account {
    fn total(&self) -> Currency {
        self.available + self.held
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TransactionKind {
    Deposit,
    Withdrawal,
}

#[derive(Debug, Clone)]
struct Transaction {
    client_id: ClientID,
    kind: TransactionKind,
    amount: Currency,
    disputed: bool,
}

#[derive(Default)]
struct Engine {
    accounts: HashMap<ClientID, Account>,
    transactions: HashMap<TransactionID, Transaction>,
}

impl Engine {
    fn apply(&mut self, row: InputRow) {
        let transaction_type = row.transaction_type.trim().to_ascii_lowercase();
        match transaction_type.as_str() {
            "deposit" => self.deposit(row.client_id, row.transaction_id, row.amount),
            "withdrawal" => self.withdrawal(row.client_id, row.transaction_id, row.amount),
            "dispute" => self.dispute(row.client_id, row.transaction_id),
            "resolve" => self.resolve(row.client_id, row.transaction_id),
            "chargeback" => self.chargeback(row.client_id, row.transaction_id),
            _ => {}
        }
    }

    fn is_locked(&self, client_id: ClientID) -> bool {
        self.accounts
            .get(&client_id)
            .map(|account| account.locked)
            .unwrap_or(false)
    }

    fn get_or_create_account(&mut self, client_id: ClientID) -> &mut Account {
        self.accounts
            .entry(client_id)
            .or_insert_with(Account::default)
    }

    fn deposit(
        &mut self,
        client_id: ClientID,
        transaction_id: TransactionID,
        amount: Option<String>,
    ) {
        if self.is_locked(client_id) {
            return;
        }
        if self.transactions.contains_key(&transaction_id) {
            return;
        }

        // convert from Option<string> to Decimal or return
        let amount = match amount {
            Some(s) => match parse_amount(&s) {
                Ok(v) => v,
                Err(_) => return,
            },
            None => return,
        };

        let account = self.get_or_create_account(client_id);
        account.available += amount;

        self.transactions.insert(
            transaction_id,
            Transaction {
                client_id,
                kind: TransactionKind::Withdrawal,
                amount,
                disputed: false,
            },
        );
    }

    fn withdrawal(
        &mut self,
        client_id: ClientID,
        transaction_id: TransactionID,
        amount: Option<String>,
    ) {
        if self.is_locked(client_id) {
            return;
        }
        if self.transactions.contains_key(&transaction_id) {
            return;
        }

        // convert from Option<string> to Decimal or return
        let amount = match amount {
            Some(s) => match parse_amount(&s) {
                Ok(v) => v,
                Err(_) => return,
            },
            None => return,
        };

        let account = self.get_or_create_account(client_id);
        if account.available < amount {
            // explicit requirement from the spec
            return;
        }
        account.available -= amount;

        self.transactions.insert(
            transaction_id,
            Transaction {
                client_id,
                kind: TransactionKind::Withdrawal,
                amount,
                disputed: false,
            },
        );
    }

    fn dispute(&mut self, client_id: ClientID, transaction_id: TransactionID) {
        if self.is_locked(client_id) {
            return;
        }
        let amount = {
            let t = match self.transactions.get(&transaction_id) {
                Some(t) => t,
                None => return,
            };
            // check if client mismatch, not a deposit, or already disputed
            if t.client_id != client_id || t.kind != TransactionKind::Deposit || t.disputed {
                return;
            }
            t.amount
        };

        let account = self.get_or_create_account(client_id);
        account.available -= amount;
        account.held += amount;

        if let Some(t) = self.transactions.get_mut(&transaction_id) {
            t.disputed = true;
        }
    }

    fn resolve(&mut self, client_id: ClientID, transaction_id: TransactionID) {
        if self.is_locked(client_id) {
            return;
        }

        let amount = {
            let t = match self.transactions.get(&transaction_id) {
                Some(t) => t,
                None => return,
            };
            // check if client mismatch, not a deposit, or transaction not disputed
            if t.client_id != client_id || t.kind != TransactionKind::Deposit || !t.disputed {
                return;
            }
            t.amount
        };

        let account = self.get_or_create_account(client_id);
        if account.held < amount {
            return;
        }
        account.held -= amount;
        account.available += amount;

        if let Some(t) = self.transactions.get_mut(&transaction_id) {
            t.disputed = false;
        }
    }

    fn chargeback(&mut self, client_id: ClientID, transaction_id: TransactionID) {
        if self.is_locked(client_id) {
            return;
        }
        let amount = {
            let t = match self.transactions.get(&transaction_id) {
                Some(t) => t,
                None => return,
            };
            // check if client mismatch, not a deposit, or transaction not disputed
            if t.client_id != client_id || t.kind != TransactionKind::Deposit || !t.disputed {
                return;
            }
            t.amount
        };

        let account = self.get_or_create_account(client_id);
        if account.held < amount {
            return;
        }
        account.held -= amount;
        account.locked = true;

        if let Some(t) = self.transactions.get_mut(&transaction_id) {
            t.disputed = false;
        }
    }
}

fn parse_amount(amount: &str) -> Result<Currency, String> {
    let t = amount.trim();
    if t.is_empty() {
        return Err("empty amount".to_string());
    }
    let mut d = Decimal::from_str(t).map_err(|_| "bad amount".to_string())?;

    // reject zero or negative amounts
    if d <= Decimal::ZERO {
        return Err("amount must be positive".to_string());
    }

    // Enforce max 4 decimal places.
    // If input has more, we fail rather than silently round, to avoid spec ambiguity.
    if d.scale() > 4 {
        return Err("too many decimal places".to_string());
    }

    // Normalize to exactly 4 dp for stable output.
    d = d.round_dp(4);
    Ok(d)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = env::args().nth(1).ok_or("Please provide a CSV file path")?;
    let file = File::open(path)?;

    let mut csv_reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .flexible(true)
        .from_reader(file);

    let mut engine = Engine::default();

    for record in csv_reader.deserialize::<InputRow>() {
        if let Ok(row) = record {
            engine.apply(row);
        }
    }

    let mut wtr = csv::Writer::from_writer(io::stdout());
    wtr.write_record(["client", "available", "held", "total", "locked"])?;

    let mut clients: Vec<ClientID> = engine.accounts.keys().copied().collect();
    clients.sort();

    for client in clients {
        let acc = &engine.accounts[&client];

        // keep output deterministic with exactly 4 decimal places
        let fmt = |d: Currency| d.round_dp(4).to_string();

        wtr.write_record([
            client.to_string(),
            fmt(acc.available),
            fmt(acc.held),
            fmt(acc.total()),
            acc.locked.to_string(),
        ])?;
    }
    wtr.flush()?;

    Ok(())
}
