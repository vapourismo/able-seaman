mod identifier;
mod k8s;
mod manager;
mod meta;
mod release;
mod utils;

use clap::Clap;
use kube::Resource;
use std::io;
use std::path::Path;

#[derive(Clap, Clone, Debug)]
enum Command {
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

    #[clap(about = "Verify a release.")]
    Verify {
        #[clap(about = "Identifier of the release")]
        release_name: String,
    },
}

#[derive(Clap, Clone, Debug)]
struct Options {
    #[clap(short, long)]
    namespace: Option<String>,

    #[clap(subcommand)]
    command: Command,
}

fn ingest_from_file_args<F: IntoIterator<Item = String>>(
    files: F,
) -> Result<release::Builder, release::BuildError> {
    let mut builder = release::Builder::new();

    for ref file in files {
        if file == "-" {
            builder.add_objects(io::stdin())?;
        } else {
            builder.add_objects_from_path(Path::new(file))?;
        }
    }

    Ok(builder)
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
        Command::Deploy {
            release_name,
            input_files,
        } => {
            let release = ingest_from_file_args(input_files)?.finish(release_name);

            let ns_mode = manager::NamespaceMode::new(options.namespace);
            let manager = manager::Manager::new(ns_mode).await?;
            let result = manager.deploy(&release).await?;

            match result {
                manager::DeployResult::Unchanged => {
                    println!("Release is unchanged.");
                }

                manager::DeployResult::Installed { plan } => {
                    println!("Release was installed.");
                    print_pretty_release_plan(&plan);
                }

                manager::DeployResult::Upgraded { plan } => {
                    println!("Release was upgraded.");
                    print_pretty_release_plan(&plan);
                }
            }
        }

        Command::Delete { release_name } => {
            let ns_mode = manager::NamespaceMode::new(options.namespace);
            let manager = manager::Manager::new(ns_mode).await?;
            let possible_plan = manager.delete(release_name).await?;

            if let Some(plan) = possible_plan {
                print_pretty_release_plan(&plan);
            }
        }

        Command::Verify { release_name } => {
            let ns_mode = manager::NamespaceMode::new(options.namespace);
            let manager = manager::Manager::new(ns_mode).await?;
            manager.verify(release_name).await?;
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
    BuildError(release::BuildError),
    ManagerError(manager::Error),
    VerificationError(Box<manager::VerificationError>),
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

impl From<release::BuildError> for GeneralError {
    fn from(error: release::BuildError) -> GeneralError {
        GeneralError::BuildError(error)
    }
}

impl From<manager::Error> for GeneralError {
    fn from(error: manager::Error) -> GeneralError {
        GeneralError::ManagerError(error)
    }
}

impl From<manager::VerificationError> for GeneralError {
    fn from(error: manager::VerificationError) -> GeneralError {
        GeneralError::VerificationError(Box::new(error))
    }
}
