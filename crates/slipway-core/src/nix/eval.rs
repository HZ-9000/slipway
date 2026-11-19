use std::sync::Arc;

use anyhow::{Context as _, Result};
use nix_bindings::{Context, Store, nix_version};

#[must_use]
pub fn linked_version() -> &'static str {
    nix_version()
}

pub struct Nix {
    context: Arc<Context>,
    store: Store,
}

impl Nix {
    pub fn open() -> Result<Self> {
        Self::open_uri(None)
    }

    #[allow(
        clippy::arc_with_non_send_sync,
        reason = "Store::open requires &Arc<Context>; Context is Send, not Sync"
    )]
    pub fn open_uri(uri: Option<&str>) -> Result<Self> {
        let context = Arc::new(Context::new().context("creating Nix context")?);
        let store = Store::open(&context, uri).context("opening Nix store")?;
        Ok(Self { context, store })
    }

    pub fn store_version(&self) -> Result<String> {
        self.store.version().context("querying store version")
    }

    #[must_use]
    pub fn store(&self) -> &Store {
        &self.store
    }

    #[must_use]
    pub fn context(&self) -> &Arc<Context> {
        &self.context
    }
}

/// The experimental features this machine's Nix settings turn on (from
/// `nix.conf` and `NIX_CONFIG`), such as `nix-command` and `flakes`.
pub fn experimental_features() -> Result<Vec<String>> {
    let context = Context::new().context("creating Nix context")?;
    let features = context
        .get_setting("experimental-features")
        .context("reading the experimental-features setting")?;
    Ok(features.split_whitespace().map(str::to_owned).collect())
}
