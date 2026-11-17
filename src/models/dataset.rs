use crate::models::account::Account;
use crate::models::card::Card;
use crate::models::customer::Customer;
use crate::models::fraud_metadata::FraudMetadata;
use crate::models::transaction::Transaction;

/// A generated dataset: the five tables in memory, before writing.
#[derive(Debug, Default)]
pub struct Dataset {
    pub customers: Vec<Customer>,
    pub accounts: Vec<Account>,
    pub cards: Vec<Card>,
    pub transactions: Vec<Transaction>,
    pub fraud_metadata: Vec<FraudMetadata>,
}
