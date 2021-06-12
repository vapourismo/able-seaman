mod errors;
mod k8s;
mod meta;
mod release;

use crate::errors::GeneralError;
use crate::release::Manager;
use crate::release::Objects;
use crate::release::Release;
use crate::release::ReleaseInfo;
use clap::Clap;
use kube::Client;
use std::path::Path;

#[derive(Clap, Clone, Debug)]
enum Command {
    Package {
        release_name: String,
        input_files: Vec<String>,
    },
    Deploy {
        release_name: String,
        input_files: Vec<String>,
    },
}

#[derive(Clap, Clone, Debug)]
struct Options {
    #[clap(subcommand)]
    command: Command,
}

#[tokio::main]
async fn main() -> Result<(), GeneralError> {
    let options = Options::parse();

    match options.command {
        Command::Package {
            release_name,
            input_files,
        } => {
            let mut release = Release::new(ReleaseInfo { name: release_name });

            for ref file in input_files {
                release.ingest_objects_from_path(Path::new(file))?;
            }

            println!("{}", serde_json::to_string_pretty(&release)?);
        }

        Command::Deploy {
            release_name,
            input_files,
        } => {
            let mut release = Release::new(ReleaseInfo { name: release_name });

            for ref file in input_files {
                release.ingest_objects_from_path(Path::new(file))?;
            }

            let client = Client::try_default().await?;

            let manager = Manager::new(client);
            manager.deploy(&release).await?;
        }
    }

    Ok(())
}
