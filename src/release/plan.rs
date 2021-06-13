use crate::k8s::apply_dynamic;
use crate::k8s::create_dynamic;
use crate::k8s::delete_dynamic;
use crate::k8s::ObjectType;
use crate::k8s::TaggableObject;
use crate::release::rollback;
use crate::release::DynamicError;
use crate::release::Objects;
use crate::release::ReleaseError;
use async_trait::async_trait;
use kube::core::DynamicObject;
use kube::Client;

#[derive(Clone, Debug)]
pub struct Create {
    new: DynamicObject,
}

impl rollback::Rollbackable for Create {
    fn to_rollback(&self) -> (rollback::RollbackAction, DynamicObject) {
        (rollback::RollbackAction::Delete, self.new.clone())
    }
}

#[derive(Clone, Debug)]
pub struct Upgrade {
    new: DynamicObject,
    old: DynamicObject,
}

impl rollback::Rollbackable for Upgrade {
    fn to_rollback(&self) -> (rollback::RollbackAction, DynamicObject) {
        (rollback::RollbackAction::Apply, self.old.clone())
    }
}

#[derive(Clone, Debug)]
pub struct Delete {
    old: DynamicObject,
}

impl rollback::Rollbackable for Delete {
    fn to_rollback(&self) -> (rollback::RollbackAction, DynamicObject) {
        (rollback::RollbackAction::Create, self.old.clone())
    }
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
        let mut rollback_plan = rollback::RollbackPlan::new();
        let mut rollback_client = client.clone();

        for creation in &self.creations {
            let result = create_dynamic(client, &creation.new)
                .await
                .on_err_rollback(rollback_client, &rollback_plan)
                .await?;

            client = result.result.client;
            rollback_client = result.rollback_client;

            rollback_plan.register(creation);
        }

        for upgrade in &self.upgrades {
            let result = apply_dynamic(client, &upgrade.new)
                .await
                .on_err_rollback(rollback_client, &rollback_plan)
                .await?;

            client = result.result.client;
            rollback_client = result.rollback_client;

            rollback_plan.register(upgrade);
        }

        for deletion in &self.deletions {
            let result = delete_dynamic(client, &deletion.old)
                .await
                .on_err_rollback(rollback_client, &rollback_plan)
                .await?;

            client = result.result;
            rollback_client = result.rollback_client;

            rollback_plan.register(deletion);
        }

        Ok(client)
    }
}

struct RollbackTriggerResult<T> {
    result: T,
    rollback_client: Client,
}

#[async_trait]
pub trait RollbackTrigger<T, E> {
    async fn on_err_rollback(self, client: Client, plan: &rollback::RollbackPlan) -> Result<T, E>;
}

#[async_trait]
impl<T> RollbackTrigger<RollbackTriggerResult<T>, ReleaseError> for Result<T, DynamicError>
where
    T: Send,
{
    async fn on_err_rollback(
        self,
        client: Client,
        plan: &rollback::RollbackPlan,
    ) -> Result<RollbackTriggerResult<T>, ReleaseError> {
        match self {
            Ok(result) => Ok(RollbackTriggerResult {
                result,
                rollback_client: client,
            }),

            Err(error) => {
                let rollback_result = plan.execute(client).await;
                Err(match rollback_result {
                    Ok(_) => ReleaseError::ActionError { error },
                    Err(rollback_error) => ReleaseError::RollbackError {
                        rollback_error,
                        error,
                    },
                })
            }
        }
    }
}
