mod k8s;
mod meta;
mod release;

use crate::release::manager::Manager;
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

async fn inner_main() -> Result<(), GeneralError> {
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

#[tokio::main]
async fn main() {
    match inner_main().await {
        Err(error) => {
            panic!("{:?}", error);
        }

        Ok(_) => {}
    }
}

#[derive(Debug)]
pub enum GeneralError {
    KubeError(kube::error::Error),
    IOError(std::io::Error),
    YAMLError(serde_yaml::Error),
    JSONError(serde_json::Error),
    ReleaseError(release::Error),
    IngestError(release::IngestError),
    ManagerError(release::manager::Error),
}

impl From<std::io::Error> for GeneralError {
    fn from(error: std::io::Error) -> GeneralError {
        GeneralError::IOError(error)
    }
}

impl From<serde_yaml::Error> for GeneralError {
    fn from(error: serde_yaml::Error) -> GeneralError {
        GeneralError::YAMLError(error)
    }
}

impl From<serde_json::Error> for GeneralError {
    fn from(error: serde_json::Error) -> GeneralError {
        GeneralError::JSONError(error)
    }
}

impl From<kube::error::Error> for GeneralError {
    fn from(error: kube::Error) -> GeneralError {
        GeneralError::KubeError(error)
    }
}

impl From<release::Error> for GeneralError {
    fn from(error: release::Error) -> GeneralError {
        GeneralError::ReleaseError(error)
    }
}

impl From<release::IngestError> for GeneralError {
    fn from(error: release::IngestError) -> GeneralError {
        GeneralError::IngestError(error)
    }
}

impl From<release::manager::Error> for GeneralError {
    fn from(error: release::manager::Error) -> GeneralError {
        GeneralError::ManagerError(error)
    }
}
