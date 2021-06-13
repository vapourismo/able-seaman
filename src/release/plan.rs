use crate::k8s::apply_dynamic;
use crate::k8s::create_dynamic;
use crate::k8s::delete_dynamic;
use crate::k8s::ObjectType;
use crate::k8s::TaggableObject;
use crate::release::DynamicError;
use crate::release::Objects;
use crate::release::ReleaseError;
use kube::core::DynamicObject;
use kube::Client;

#[derive(Clone, Debug)]
struct Create {
    new: DynamicObject,
}

#[derive(Clone, Debug)]
struct Upgrade {
    new: DynamicObject,
    old: DynamicObject,
}

#[derive(Clone, Debug)]
struct Delete {
    old: DynamicObject,
}

#[derive(Clone, Debug)]
pub struct ReleasePlan {
    creations: Vec<Create>,
    upgrades: Vec<Upgrade>,
    deletions: Vec<Delete>,
}

impl ReleasePlan {
    pub fn new(new_objects: &Objects, old_objects: &Objects) -> Self {
        // Find things to create.
        let creations = new_objects
            .iter()
            .filter(|(key, _)| !old_objects.contains_key(*key))
            .map(|(_, new)| Create {
                new: new.to_tagged(ObjectType::Managed),
            })
            .collect();

        // Find things to upgrade.
        let upgrades = new_objects
            .iter()
            .filter_map(|(key, new)| {
                old_objects.get(key).map(|current| Upgrade {
                    new: new.to_tagged(ObjectType::Managed),
                    old: current.to_tagged(ObjectType::Managed),
                })
            })
            .collect();

        // Find things to delete.
        let deletions = old_objects
            .iter()
            .filter(|(key, _)| !new_objects.contains_key(*key))
            .map(|(_, value)| Delete {
                old: value.to_tagged(ObjectType::Managed),
            })
            .collect();

        ReleasePlan {
            creations,
            upgrades,
            deletions,
        }
    }

    pub async fn execute(&self, mut client: Client) -> Result<Client, ReleaseError> {
        let rollback_client = client.clone();
        let mut rollback_creations = Vec::new();
        let mut rollback_upgrades = Vec::new();
        let mut rollback_deletions = Vec::new();

        for creation in &self.creations {
            let result = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                create_dynamic(client, &creation.new).await,
            )
            .await?;
            client = result.client;
            rollback_deletions.push(&creation.new);
        }

        for upgrade in &self.upgrades {
            let result = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                apply_dynamic(client, &upgrade.new).await,
            )
            .await?;
            client = result.client;
            rollback_upgrades.push(&upgrade.old);
        }

        for deletion in &self.deletions {
            client = or_rollback(
                rollback_client.clone(),
                &rollback_creations,
                &rollback_upgrades,
                &rollback_deletions,
                delete_dynamic(client, &deletion.old).await,
            )
            .await?;
            rollback_creations.push(&deletion.old);
        }

        Ok(client)
    }
}

#[derive(Debug)]
pub enum RollbackError {
    DynamicError(DynamicError),
}

impl From<DynamicError> for RollbackError {
    fn from(error: DynamicError) -> Self {
        RollbackError::DynamicError(error)
    }
}

async fn rollback(
    mut client: Client,
    creations: &Vec<&DynamicObject>,
    upgrades: &Vec<&DynamicObject>,
    deletions: &Vec<&DynamicObject>,
) -> Result<(), RollbackError> {
    for creation in creations {
        client = create_dynamic(client, creation).await?.client;
    }

    for upgrade in upgrades {
        client = apply_dynamic(client, upgrade).await?.client;
    }

    for deletion in deletions {
        client = delete_dynamic(client, deletion).await?;
    }

    Ok(())
}

async fn or_rollback<T>(
    client: Client,
    creations: &Vec<&DynamicObject>,
    upgrades: &Vec<&DynamicObject>,
    deletions: &Vec<&DynamicObject>,
    result: Result<T, DynamicError>,
) -> Result<T, ReleaseError> {
    match result {
        Err(error) => {
            let rollback_result = rollback(client, &creations, &upgrades, &deletions).await;
            Err(match rollback_result {
                Ok(_) => ReleaseError::ActionError { error },
                Err(rollback_error) => ReleaseError::RollbackError {
                    rollback_error,
                    error,
                },
            })
        }

        Ok(result) => Ok(result),
    }
}
