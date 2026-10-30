use anyhow::{Result, anyhow};
use nostr_connect::prelude::*;

#[derive(Debug, Clone)]
pub enum Signer {
    ReadOnly(PublicKey),
    Local(Keys),
    Bunker(Box<NostrConnect>),
}

impl Signer {
    pub async fn public_key(&self) -> Result<PublicKey> {
        match self {
            Self::ReadOnly(public_key) => Ok(*public_key),
            Self::Local(keys) => Ok(keys.public_key()),
            Self::Bunker(connect) => connect
                .get_public_key_async()
                .await
                .map_err(|err| anyhow!(err)),
        }
    }

    pub async fn sign(&self, builder: EventBuilder) -> Result<Event> {
        match self {
            Self::ReadOnly(_) => Err(anyhow!("this account is read-only")),
            Self::Local(keys) => Ok(builder.finalize(keys)?),
            Self::Bunker(connect) => Ok(builder.finalize_async(connect.as_ref()).await?),
        }
    }
}

pub fn parse_public_key(input: &str) -> Result<PublicKey> {
    let input = input.trim();
    if input.starts_with("nprofile")
        && let Nip19::Profile(profile) = Nip19::from_bech32(input)?
    {
        return Ok(profile.public_key);
    }
    Ok(PublicKey::parse(input)?)
}
