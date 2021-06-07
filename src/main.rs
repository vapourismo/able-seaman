mod errors;
mod objects;

use crate::errors::GeneralError;
use crate::objects::ingest_objects;
use crate::objects::ReleaseInfo;
use kube::core::DynamicObject;
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

fn actual_main() -> Result<(), GeneralError> {
    let release = ReleaseInfo {
        name: "example_release".to_string(),
    };

    let mut objects: BTreeMap<String, DynamicObject> = BTreeMap::new();

    objects.append(&mut ingest_objects(&release, file_reader("pod.yaml")?)?);

    for (k, v) in objects {
        println!("> {}", k);
        println!("{}", serde_json::to_string_pretty(&v)?);
    }

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
