use anyhow::{Context, Result};
use keyring::Entry;

const SERVICE: &str = "rastrich";

fn entry(username: &str) -> Result<Entry> {
    Entry::new(SERVICE, username).context("could not open system keyring")
}

pub fn store_nsec(npub: &str, nsec: &str) -> Result<()> {
    entry(&format!("nsec:{npub}"))?
        .set_password(nsec)
        .context("could not store nsec in keyring")
}

pub fn load_nsec(npub: &str) -> Result<Option<String>> {
    match entry(&format!("nsec:{npub}"))?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(err) => Err(err).context("could not read nsec from keyring"),
    }
}

pub fn store_bunker(npub: &str, bunker_uri: &str, app_nsec: &str) -> Result<()> {
    entry(&format!("bunker:{npub}"))?
        .set_password(bunker_uri)
        .context("could not store bunker URI in keyring")?;
    entry(&format!("bunker-app:{npub}"))?
        .set_password(app_nsec)
        .context("could not store bunker app key in keyring")?;
    Ok(())
}

pub fn load_bunker(npub: &str) -> Result<Option<(String, String)>> {
    let uri = match entry(&format!("bunker:{npub}"))?.get_password() {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(err) => return Err(err).context("could not read bunker URI from keyring"),
    };
    let app_nsec = match entry(&format!("bunker-app:{npub}"))?.get_password() {
        Ok(value) => value,
        Err(keyring::Error::NoEntry) => return Ok(None),
        Err(err) => return Err(err).context("could not read bunker app key from keyring"),
    };
    Ok(Some((uri, app_nsec)))
}
