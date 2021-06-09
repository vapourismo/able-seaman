use crate::errors::GeneralError;
use crate::objects::attach_annotations;
use crate::objects::attach_labels;
use kube::core::DynamicObject;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::hash::Hash;
use std::io::Read;

#[derive(PartialOrd, Ord, PartialEq, Eq, Hash, Clone, Debug)]
pub struct ReleaseInfo {
    pub name: String,
}

impl ReleaseInfo {
    pub fn configure_object(&self, object: &mut DynamicObject) {
        attach_labels(object, self.name.clone());
        attach_annotations(object);
    }
}

#[derive(Clone, Debug)]
pub struct Release {
    pub info: ReleaseInfo,
    pub objects: Objects,
}

impl Release {
    pub fn new(info: ReleaseInfo) -> Self {
        Release {
            info,
            objects: BTreeMap::new(),
        }
    }

    pub fn ingest_objects<SomeRead>(&mut self, input: SomeRead) -> Result<(), GeneralError>
    where
        SomeRead: Read,
    {
        self.objects.append(&mut ingest_objects(&self.info, input)?);

        Ok(())
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

pub type Objects = BTreeMap<String, DynamicObject>;

pub fn ingest_objects<SomeRead>(
    release: &ReleaseInfo,
    input: SomeRead,
) -> Result<Objects, GeneralError>
where
    SomeRead: std::io::Read,
{
    let mut objects = BTreeMap::new();

    for document in serde_yaml::Deserializer::from_reader(input) {
        let mut object = DynamicObject::deserialize(document)?;

        if let Some(name) = object.metadata.name.clone() {
            release.configure_object(&mut object);

            if let Some(_old_object) = objects.insert(name.clone(), object) {
                return Err(GeneralError::DuplicateObject(name));
            }
        } else {
            return Err(GeneralError::ObjectWithoutName(object));
        }
    }

    Ok(objects)
}
