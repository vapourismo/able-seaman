mod errors;
mod objects;
mod release;

use crate::errors::GeneralError;
use crate::release::Release;
use crate::release::ReleaseInfo;
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

fn actual_main() -> Result<(), GeneralError> {
    let release = ReleaseInfo {
        name: "example_release".to_string(),
    };

    let mut release = Release::new(release);

    let input = file_reader("pod.yaml")?;
    release.ingest_objects(input)?;

    Ok(())
}

fn main() {
    match actual_main() {
        Ok(()) => {}
        Err(error) => {
            panic!("{:?}", error);
        }
    }
}

// #[tokio::main]
// async fn main() -> Result<(), GeneralError> {
//     Ok(())
// }
