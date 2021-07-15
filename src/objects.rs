use crate::identifier::Identifier;
use crate::k8s::api_resource::ToApiResource;
use crate::k8s::api_resource::TryToApiResource;
use kube::core::ApiResource;
use kube::core::DynamicObject;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use std::borrow::Cow;
use std::collections::hash_map;
use std::collections::HashMap;
use std::convert::TryFrom;

/// Clone of ApiResource that supports Serialize and Deserialize
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(remote = "ApiResource")]
struct SerDeApiResource {
    group: String,
    version: String,
    api_version: String,
    kind: String,
    plural: String,
}

/// A deployable object
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Object {
    #[serde(with = "SerDeApiResource")]
    pub api_resource: ApiResource,

    pub dyn_object: DynamicObject,
}

impl Object {
    /// Try to convert a DynamicObject into an Object.
    pub fn try_from_dynamic_object(dyn_object: DynamicObject) -> Option<Self> {
        dyn_object.try_to_api_resource().map(|api_resource| Object {
            api_resource,
            dyn_object,
        })
    }

    /// Get the name of the underlying Object.
    pub fn name(&self) -> Option<&String> {
        self.dyn_object.metadata.name.as_ref()
    }
}

impl ToApiResource for Object {
    fn to_api_resource(&self) -> ApiResource {
        self.api_resource.clone()
    }
}

impl TryFrom<DynamicObject> for Object {
    type Error = String;

    fn try_from(dyn_object: DynamicObject) -> Result<Self, Self::Error> {
        Object::try_from_dynamic_object(dyn_object)
            .ok_or_else(|| "Unable to extract ApiResource from DynamicObject".to_string())
    }
}

impl kube::Resource for Object {
    type DynamicType = <DynamicObject as kube::Resource>::DynamicType;

    fn kind(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::kind(dt)
    }

    fn group(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::group(dt)
    }

    fn version(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::version(dt)
    }

    fn plural(dt: &Self::DynamicType) -> Cow<'_, str> {
        <DynamicObject as kube::Resource>::plural(dt)
    }

    fn meta(&self) -> &kube::core::ObjectMeta {
        self.dyn_object.meta()
    }

    fn meta_mut(&mut self) -> &mut kube::core::ObjectMeta {
        self.dyn_object.meta_mut()
    }
}

#[derive(Debug, Clone)]
pub struct Objects {
    inner: HashMap<Identifier, Object>,
}

impl Objects {
    pub fn new() -> Self {
        Objects {
            inner: HashMap::new(),
        }
    }

    pub fn contains(&self, ident: &Identifier) -> bool {
        self.inner.contains_key(ident)
    }

    pub fn iter(&self) -> hash_map::Iter<'_, Identifier, Object> {
        self.inner.iter()
    }

    pub fn get(&self, key: &Identifier) -> Option<&Object> {
        self.inner.get(key)
    }

    pub fn insert(&mut self, key: Identifier, value: Object) -> Option<Object> {
        self.inner.insert(key, value)
    }
}

impl Default for Objects {
    fn default() -> Self {
        Self::new()
    }
}

impl IntoIterator for Objects {
    type Item = <HashMap<Identifier, Object> as IntoIterator>::Item;

    type IntoIter = <HashMap<Identifier, Object> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<'a> IntoIterator for &'a Objects {
    type Item = <&'a HashMap<Identifier, Object> as IntoIterator>::Item;

    type IntoIter = <&'a HashMap<Identifier, Object> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        <&'a HashMap<Identifier, Object> as IntoIterator>::into_iter(&self.inner)
    }
}

impl From<HashMap<Identifier, Object>> for Objects {
    fn from(inner: HashMap<Identifier, Object>) -> Self {
        Objects { inner }
    }
}

impl From<Objects> for HashMap<Identifier, Object> {
    fn from(other: Objects) -> Self {
        other.inner
    }
}

#[derive(Serialize, Deserialize)]
struct SerDeObjectsEntry {
    identifier: Identifier,
    object: Object,
}

impl Serialize for Objects {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.inner
            .iter()
            .map(|(identifier, object)| SerDeObjectsEntry {
                identifier: identifier.clone(),
                object: object.clone(),
            })
            .collect::<Vec<SerDeObjectsEntry>>()
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Objects {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let inner = Vec::deserialize(deserializer)?
            .into_iter()
            .map(|entry: SerDeObjectsEntry| (entry.identifier, entry.object))
            .collect();

        Ok(Objects { inner })
    }
}
