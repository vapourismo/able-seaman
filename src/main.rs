mod errors;
mod objects;
mod release;
mod resources;

use crate::errors::GeneralError;
use crate::release::Release;
use crate::release::ReleaseInfo;
use crate::resources::get_api_resources;
use crate::resources::get_core_api_resources;
use crate::resources::list_release_resources;
use kube::Client;
use std::ffi::OsStr;
use std::fs::File;
use std::io::Read;
use std::path::Path;

fn file_reader<SomeOsStr>(path: SomeOsStr) -> Result<Box<dyn Read>, GeneralError>
where
    SomeOsStr: AsRef<OsStr>,
{
    Ok(Box::new(File::open(Path::new(&path))?))
}

fn load_release(info: &ReleaseInfo) -> Result<Release, GeneralError> {
    let mut release = Release::new(info.clone());

    let input = file_reader("pod.yaml")?;
    release.ingest_objects(input)?;

    Ok(release)
}

#[tokio::main]
async fn main() -> Result<(), GeneralError> {
    let info = ReleaseInfo {
        name: "example_release".to_string(),
    };

    let client = Client::try_default().await?;

    let mut core_api_resources = get_core_api_resources(&client).await?;
    core_api_resources.append(&mut get_api_resources(&client).await?);

    for car in core_api_resources {
        let objects = list_release_resources(&client, &car, &info).await?;
        for o in objects {
            println!("{:?}", o);
        }
    }

    Ok(())
}
