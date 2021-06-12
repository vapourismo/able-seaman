pub mod lock;
pub use lock::*;

pub mod api_resource;
pub use api_resource::*;

use crate::meta::CRATE_VERSION;
use futures::StreamExt;
use futures::TryStreamExt;
use kube::api::ListParams;
use kube::api::WatchEvent;
use kube::Api;
use kube::ResourceExt;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

const TYPE_LABEL: &'static str = "able-seaman/type";
const VERSION_LABEL: &'static str = "able-seaman/version";

#[derive(Clone, Copy, Debug, Serialize)]
pub enum ObjectType {
    Lock,
    Release,
}

impl ToString for ObjectType {
    fn to_string(&self) -> String {
        match self {
            ObjectType::Lock => "lock",
            ObjectType::Release => "release",
        }
        .to_string()
    }
}

pub fn tag_object<SomeResource>(object: &mut SomeResource, object_type: ObjectType)
where
    SomeResource: ResourceExt,
{
    object
        .labels_mut()
        .insert(TYPE_LABEL.to_string(), object_type.to_string());

    object
        .annotations_mut()
        .insert(VERSION_LABEL.to_string(), CRATE_VERSION.to_string());
}

async fn wait_for_deletion<SomeResource>(
    api: &Api<SomeResource>,
    name: &String,
) -> Result<(), kube::Error>
where
    SomeResource: Clone + DeserializeOwned + Debug + ResourceExt,
{
    let mut stream = api
        .watch(
            &ListParams::default()
                .labels(format!("{}=lock", TYPE_LABEL).as_str())
                .timeout(10),
            "0",
        )
        .await?
        .boxed();

    while let Some(event) = stream.try_next().await? {
        match event {
            WatchEvent::Deleted(deletion) if &deletion.name() == name => {
                return Ok(());
            }

            _ => {}
        }
    }

    Ok(())
}
