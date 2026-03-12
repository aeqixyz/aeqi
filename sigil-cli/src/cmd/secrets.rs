use anyhow::Result;
use sigil_core::SecretStore;
use std::path::PathBuf;

use crate::cli::SecretsAction;
use crate::helpers::load_config;

pub(crate) async fn cmd_secrets(
    config_path: &Option<PathBuf>,
    action: SecretsAction,
) -> Result<()> {
    let store_path = if let Ok((config, _)) = load_config(config_path) {
        config
            .security
            .secret_store
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| config.data_dir().join("secrets"))
    } else {
        dirs::home_dir().unwrap_or_default().join(".sigil/secrets")
    };
    let store = SecretStore::open(&store_path)?;

    match action {
        SecretsAction::Set { name, value } => {
            store.set(&name, &value)?;
            println!("Secret '{name}' stored.");
        }
        SecretsAction::Get { name } => println!("{}", store.get(&name)?),
        SecretsAction::List => {
            let names = store.list()?;
            if names.is_empty() {
                println!("No secrets stored.");
            } else {
                for n in names {
                    println!("  {n}");
                }
            }
        }
        SecretsAction::Delete { name } => {
            store.delete(&name)?;
            println!("Secret '{name}' deleted.");
        }
    }
    Ok(())
}
