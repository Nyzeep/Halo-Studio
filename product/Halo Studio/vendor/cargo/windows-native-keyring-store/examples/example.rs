//! Example CLI app that creates, writes, reads, examines, and deletes an entry
//! in the Windows credential manager using APIs from the keyring crate.
use std::collections::HashMap;

use keyring_core::Entry;
use windows_native_keyring_store::Store;

fn main() {
    // Set Windows credential manager as the default store
    keyring_core::set_default_store(Store::new().unwrap());

    let service = "service";
    let username = "user";
    let password = "<PASSWORD>";
    let entry = Entry::new(service, username).unwrap();
    entry.set_password(password).unwrap();
    let retrieved = entry.get_password().unwrap();
    if retrieved != password {
        panic!("Passwords do not match");
    }
    println!("Entry with no target: {entry:?}");
    entry.delete_credential().unwrap();
    let modifiers = HashMap::from([("target", "custom.target")]);
    let entry = Entry::new_with_modifiers("service", "user", &modifiers).unwrap();
    entry.set_password(password).unwrap();
    let retrieved = entry.get_password().unwrap();
    if retrieved != password {
        panic!("Passwords do not match");
    }
    println!("Entry with custom target: {entry:?}");
    entry.delete_credential().unwrap();
    keyring_core::unset_default_store();
}
