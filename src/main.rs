mod errors;
mod objects;
mod release;
mod resources;

use crate::errors::GeneralError;
use crate::release::Release;
use crate::release::ReleaseInfo;
use crate::resources::list_release_resources;
use crate::resources::ApiKnowledge;
use kube::Client;
use std::collections::BTreeMap;
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

fn load_release(info: ReleaseInfo) -> Result<Release, GeneralError> {
    let mut release = Release::new(info);

    let input = file_reader("pod.yaml")?;
    release.ingest_objects(input)?;

    Ok(release)
}

#[tokio::main]
async fn main() -> Result<(), GeneralError> {
    let release = load_release(ReleaseInfo {
        name: "example_release".to_string(),
    })?;

    let client = Client::try_default().await?;
    let knowledge = ApiKnowledge::new(&client).await?;

    let (_client, objects) = list_release_resources(client, &knowledge, &release.info).await?;

    dbg!(release);
    dbg!(objects);

    Ok(())
}
