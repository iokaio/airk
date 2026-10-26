// SPDX-License-Identifier: Apache-2.0
//! Generate a compact empty ledger image without application/user state.
use redb::{Durability, TableDefinition};
use std::{env, fs, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let output = PathBuf::from(env::var_os("OUT_DIR").ok_or("OUT_DIR missing")?);
    let temporary = tempfile::NamedTempFile::new_in(&output)?;
    let mut database = redb::Database::builder().create_file(temporary.reopen()?)?;
    let mut transaction = database.begin_write()?;
    transaction.set_durability(Durability::Immediate)?;
    transaction.set_two_phase_commit(true);
    transaction.open_table(TableDefinition::<&str, &[u8]>::new("ledger"))?;
    transaction.commit()?;
    for _ in 0..8 {
        if !database.compact()? {
            break;
        }
    }
    drop(database);
    let bytes = fs::read(temporary.path())?;
    if bytes.len() > 65536 {
        return Err("empty redb ledger cannot fit the existing 64 KiB minimum policy".into());
    }
    let mut image = fs::File::create(output.join("empty-ledger.airk"))?;
    image.write_all(&bytes)?;
    image.sync_all()?;
    Ok(())
}
