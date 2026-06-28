use gloo_storage::{LocalStorage, Storage};

const REMEMBERED_ACCOUNTS_KEY: &str = "denpie.remembered_accounts";
const REMEMBERED_MAX: usize = 5;

pub fn load_remembered() -> Vec<String> {
    LocalStorage::get::<Vec<String>>(REMEMBERED_ACCOUNTS_KEY).unwrap_or_default()
}

pub fn save_remembered(accounts: &[String]) {
    let _ = LocalStorage::set(REMEMBERED_ACCOUNTS_KEY, accounts);
}

pub fn record_account(name: &str) {
    if name.is_empty() {
        return;
    }
    let mut accounts = load_remembered();
    accounts.retain(|a| a != name);
    accounts.insert(0, name.to_string());
    if accounts.len() > REMEMBERED_MAX {
        accounts.truncate(REMEMBERED_MAX);
    }
    save_remembered(&accounts);
}
