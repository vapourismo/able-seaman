mod k8s;
mod meta;
mod release;

use clap::Clap;
use kube::Client;
use kube::Resource;
use std::io;
use std::path::Path;

#[derive(Clap, Clone, Debug)]
enum Command {
    #[clap(about = "Package some objects into a release description.")]
    Package {
        #[clap(about = "Identifier of the release")]
        release_name: String,
        #[clap(
            about = "Files or entire directories from which the Kubernetes objects should be read from (you can use '-' to read objects from stdin)"
        )]
        input_files: Vec<String>,
    },

    #[clap(about = "Deploy a release.")]
    Deploy {
        #[clap(about = "Identifier of the release")]
        release_name: String,
        #[clap(
            about = "Files or entire directories from which the Kubernetes objects should be read from (you can use '-' to read objects from stdin)"
        )]
        input_files: Vec<String>,
    },

    #[clap(about = "Delete a release.")]
    Delete {
        #[clap(about = "Identifier of the release")]
        release_name: String,
    },
}

#[derive(Clap, Clone, Debug)]
struct Options {
    #[clap(subcommand)]
    command: Command,
}

fn ingest_from_file_args<F: IntoIterator<Item = String>>(
    release: &mut release::Release,
    files: F,
) -> Result<(), release::IngestError> {
    for ref file in files {
        if file == "-" {
            release.ingest_objects(io::stdin())?;
        } else {
            release.ingest_objects_from_path(Path::new(file))?;
        }
    }

    Ok(())
}

fn print_pretty_release_plan(plan: &release::plan::ReleasePlan) {
    if !plan.creations.is_empty() {
        println!("Creations: {}", plan.creations.len());
        for creation in &plan.creations {
            if let Some(name) = &creation.new.meta().name {
                println!("+ {}", name)
            }
        }
    }

    if !plan.upgrades.is_empty() {
        println!("Upgrades: {}", plan.upgrades.len());
        for upgrade in &plan.upgrades {
            if let Some(name) = &upgrade.new.meta().name {
                println!("~ {}", name)
            }
        }
    }

    if !plan.deletions.is_empty() {
        println!("Deletions: {}", plan.deletions.len());
        for deletion in &plan.deletions {
            if let Some(name) = &deletion.old.meta().name {
                println!("- {}", name)
            }
        }
    }
}

async fn inner_main() -> Result<(), GeneralError> {
    let options = Options::parse();

    match options.command {
        Command::Package {
            release_name,
            input_files,
        } => {
            let mut release = release::Release::new(release::ReleaseInfo { name: release_name });
            ingest_from_file_args(&mut release, input_files)?;

            println!("{}", serde_json::to_string_pretty(&release)?);
        }

        Command::Deploy {
            release_name,
            input_files,
        } => {
            let mut release = release::Release::new(release::ReleaseInfo { name: release_name });
            ingest_from_file_args(&mut release, input_files)?;

            let client = Client::try_default().await?;
            let manager = release::manager::Manager::new(client);
            let result = manager.deploy(&release).await?;

            match result {
                release::manager::DeployResult::Unchanged => {
                    println!("Release is unchanged.");
                }

                release::manager::DeployResult::Installed { plan } => {
                    println!("Release was installed.");
                    print_pretty_release_plan(&plan);
                }

                release::manager::DeployResult::Upgraded { plan } => {
                    println!("Release was upgraded.");
                    print_pretty_release_plan(&plan);
                }
            }
        }

        Command::Delete { release_name } => {
            let client = Client::try_default().await?;
            let manager = release::manager::Manager::new(client);
            let possible_plan = manager.delete(release_name).await?;

            if let Some(plan) = possible_plan {
                print_pretty_release_plan(&plan);
            }
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() {
    inner_main()
        .await
        .unwrap_or_else(|error| panic!("{:#?}", error))
}

#[derive(Debug)]
pub enum GeneralError {
    KubeError(kube::error::Error),
    IOError(std::io::Error),
    YAMLError(serde_yaml::Error),
    JSONError(serde_json::Error),
    ReleaseError(Box<release::Error>),
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
        GeneralError::ReleaseError(Box::new(error))
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
