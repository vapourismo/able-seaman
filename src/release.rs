pub mod manager;
pub mod plan;
pub mod rollback;

use crate::k8s::lock::Lock;
use crate::k8s::transaction;
use crate::release::plan::ReleasePlan;
use k8s_openapi::api::core::v1::ConfigMap;
use kube::core::DynamicObject;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::File;
use std::hash::Hash;
use std::io;
use std::path::{Path, PathBuf};

fn list_contents_vec(paths: &mut Vec<PathBuf>, path: &Path) -> Result<(), io::Error> {
    if path.is_dir() {
        for entry in path.read_dir()? {
            let dir = entry?.path();
            list_contents_vec(paths, dir.as_path())?;
        }
    } else if path.exists() {
        paths.push(path.to_path_buf());
    } else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            path.as_os_str().to_str().expect("BadPath"),
        ));
    }

    Ok(())
}

fn list_contents(path: &Path) -> Result<Vec<PathBuf>, io::Error> {
    let mut paths = Vec::new();
    list_contents_vec(&mut paths, path)?;
    Ok(paths)
}

#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Debug, Serialize, Deserialize)]
pub struct ReleaseInfo {
    pub name: String,
}

#[derive(Debug)]
pub enum Error {
    RollbackError {
        error: rollback::Error,
        cause: transaction::Error,
    },

    ReleaseError {
        error: transaction::Error,
    },
}

#[derive(Debug)]
pub enum IngestError {
    DuplicateObject(String),
    ObjectWithoutName(Box<DynamicObject>),
    IOError(io::Error),
    YAMLError(serde_yaml::Error),
}

impl From<serde_yaml::Error> for IngestError {
    fn from(error: serde_yaml::Error) -> IngestError {
        IngestError::YAMLError(error)
    }
}

impl From<io::Error> for IngestError {
    fn from(error: io::Error) -> IngestError {
        IngestError::IOError(error)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    info: ReleaseInfo,
    objects: Objects,
}

pub type Objects = BTreeMap<String, DynamicObject>;

impl Release {
    pub fn new(info: ReleaseInfo) -> Self {
        Release {
            info,
            objects: BTreeMap::new(),
        }
    }

    #[allow(clippy::needless_lifetimes)]
    pub async fn lock<'a>(
        &self,
        api: &'a kube::Api<ConfigMap>,
    ) -> Result<Lock<'a, ConfigMap>, kube::Error> {
        Lock::new(api, format!("{}-lock", self.info.name)).await
    }

    pub fn ingest_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), IngestError>
    where
        SomeRead: io::Read,
    {
        for document in serde_yaml::Deserializer::from_reader(input) {
            let object = DynamicObject::deserialize(document)?;

            if let Some(name) = object.metadata.name.clone() {
                if let Some(_old_object) = self.objects.insert(name.clone(), object) {
                    return Err(IngestError::DuplicateObject(name));
                }
            } else {
                return Err(IngestError::ObjectWithoutName(Box::new(object)));
            }
        }

        Ok(())
    }

    pub fn ingest_objects_from_path(&mut self, input: &Path) -> Result<(), IngestError> {
        for file in list_contents(input)? {
            let file = File::open(file.as_path())?;
            self.ingest_objects(file)?;
        }

        Ok(())
    }

    pub async fn upgrade(&self, old: &Self, client: kube::Client) -> Result<kube::Client, Error> {
        let plan = ReleasePlan::new(&self.objects, &old.objects);
        Ok(plan.execute(client).await?)
    }

    pub async fn install(&self, client: kube::Client) -> Result<kube::Client, Error> {
        let plan = ReleasePlan::new(&self.objects, &BTreeMap::new());
        Ok(plan.execute(client).await?)
    }
}

impl Hash for Release {
    fn hash<SomeHasher>(&self, hasher: &mut SomeHasher)
    where
        SomeHasher: std::hash::Hasher,
    {
        self.info.hash(hasher);

        for (name, object) in &self.objects {
            name.hash(hasher);

            match serde_json::to_string(object) {
                Ok(json) => json.hash(hasher),
                Err(_error) => {}
            }
        }
    }
}
