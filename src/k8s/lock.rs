use crate::k8s;
use crate::k8s::labels;
use futures::StreamExt;
use futures::TryStreamExt;
use kube::api;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Debug;

async fn wait_for_deletion<SomeResource>(
    api: &kube::Api<SomeResource>,
    name: &str,
) -> Result<(), kube::Error>
where
    SomeResource: Clone + DeserializeOwned + Debug + kube::ResourceExt,
{
    let mut stream = api
        .watch(
            &labels::Labels::new()
                .add(k8s::ObjectType::Lock)
                .to_listparams()
                .timeout(10),
            "0",
        )
        .await?
        .boxed();

    while let Some(event) = stream.try_next().await? {
        match event {
            api::WatchEvent::Deleted(deletion) if deletion.name() == name => {
                return Ok(());
            }

            _ => {}
        }
    }

    Ok(())
}

pub struct Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
    api: &'a kube::Api<T>,
    name: String,
    deleted: bool,
}

impl<'a, T> Lock<'a, T>
where
    T: kube::Resource + Default + Clone + Debug + DeserializeOwned + Serialize,
{
    pub async fn new(api: &'a kube::Api<T>, name: String) -> Result<Lock<'a, T>, kube::Error> {
        Lock::new_with(api, name, <T as Default>::default()).await
    }

    pub async fn new_with(
        api: &'a kube::Api<T>,
        name: String,
        mut lock_value: T,
    ) -> Result<Lock<'a, T>, kube::Error> {
        lock_value.meta_mut().name = Some(name.clone());
        labels::Labels::new()
            .add(k8s::ObjectType::Lock)
            .apply_to(&mut lock_value);

        let _locked_value = loop {
            match api.create(&api::PostParams::default(), &lock_value).await {
                Err(kube::Error::Api(kube::error::ErrorResponse {
                    reason, code: 409, ..
                })) if reason == "AlreadyExists" => {
                    wait_for_deletion(&api, &name).await?;
                }

                result => {
                    break result?;
                }
            }
        };

        Ok(Lock {
            api,
            name,
            deleted: false,
        })
    }

    pub async fn release(mut self) -> Result<(), kube::Error> {
        self.api
            .delete(self.name.as_str(), &api::DeleteParams::default())
            .await?;
        self.deleted = true;
        Ok(())
    }
}

impl<'a, T> Drop for Lock<'a, T>
where
    T: Clone + DeserializeOwned + Debug,
{
    fn drop(&mut self) {
        if self.deleted {
            return;
        }

        let deletion = futures::executor::block_on(
            self.api
                .delete(self.name.as_str(), &api::DeleteParams::default()),
        );

        if let Err(err) = deletion {
            eprintln!("Failed to delete locking ConfigMap {}: {}", self.name, err);
        }
    }
}
