mod errors;
mod objects;
mod release;
mod resources;

use crate::errors::GeneralError;
use crate::release::Release;
use crate::release::ReleaseInfo;
use std::path::Path;

#[tokio::main]
async fn main() -> Result<(), GeneralError> {
    let mut release = Release::new(ReleaseInfo {
        name: "example_release".to_string(),
    });

    release.ingest_objects_from_path(&Path::new("objects"))?;

    println!("{}", serde_yaml::to_string(&release.as_config_map()?)?);

    Ok(())
}
