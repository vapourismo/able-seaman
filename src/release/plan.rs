use crate::k8s::labels;
use crate::k8s::labels::WithLabels;
use crate::k8s::transaction;
use crate::release;
use crate::release::rollback;
use async_trait::async_trait;
use kube::core::DynamicObject;
use kube::Client;

#[derive(Clone, Debug)]
pub struct Create {
    pub(crate) new: DynamicObject,
}

impl rollback::Rollbackable for Create {
    fn to_rollback(&self) -> (transaction::Action, &DynamicObject) {
        (transaction::Action::Delete, &self.new)
    }
}

#[derive(Clone, Debug)]
pub struct Upgrade {
    pub(crate) new: DynamicObject,
    pub(crate) old: DynamicObject,
}

impl rollback::Rollbackable for Upgrade {
    fn to_rollback(&self) -> (transaction::Action, &DynamicObject) {
        (transaction::Action::Apply, &self.old)
    }
}

#[derive(Clone, Debug)]
pub struct Delete {
    pub(crate) old: DynamicObject,
}

impl rollback::Rollbackable for Delete {
    fn to_rollback(&self) -> (transaction::Action, &DynamicObject) {
        (transaction::Action::Create, &self.old)
    }
}

#[derive(Clone, Debug)]
pub struct ReleasePlan {
    pub(crate) creations: Vec<Create>,
    pub(crate) upgrades: Vec<Upgrade>,
    pub(crate) deletions: Vec<Delete>,
}

impl ReleasePlan {
    pub fn new(new_objects: &release::Objects, old_objects: &release::Objects) -> Self {
        let managed_labels = labels::Labels::new().add(labels::TypeLabel::Managed);

        // Find things to create.
        let creations = new_objects
            .iter()
            .filter(|(key, _)| !old_objects.contains_key(*key))
            .map(|(_, new)| Create {
                new: new.clone().with_labels(&managed_labels),
            })
            .collect();

        // Find things to upgrade.
        let upgrades = new_objects
            .iter()
            .filter_map(|(key, new)| {
                old_objects.get(key).map(|old| Upgrade {
                    new: new.clone().with_labels(&managed_labels),
                    old: old.clone().with_labels(&managed_labels),
                })
            })
            .collect();

        // Find things to delete.
        let deletions = old_objects
            .iter()
            .filter(|(key, _)| !new_objects.contains_key(*key))
            .map(|(_, old)| Delete {
                old: old.clone().with_labels(&managed_labels),
            })
            .collect();

        ReleasePlan {
            creations,
            upgrades,
            deletions,
        }
    }

    pub async fn execute(&self, mut client: Client) -> Result<Client, release::Error> {
        let mut rollback_plan = rollback::Plan::new();
        let mut rollback_client = client.clone();

        for creation in &self.creations {
            let result = transaction::create_dynamic(client, &creation.new)
                .await
                .on_err_rollback(rollback_client, &rollback_plan)
                .await?;

            client = result.result.client;
            rollback_client = result.rollback_client;

            rollback_plan.register(creation);
        }

        for upgrade in &self.upgrades {
            let result = transaction::apply_dynamic(client, &upgrade.new)
                .await
                .on_err_rollback(rollback_client, &rollback_plan)
                .await?;

            client = result.result.client;
            rollback_client = result.rollback_client;

            rollback_plan.register(upgrade);
        }

        for deletion in &self.deletions {
            let result = transaction::delete_dynamic(client, &deletion.old)
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
    async fn on_err_rollback(self, client: Client, plan: &rollback::Plan) -> Result<T, E>;
}

#[async_trait]
impl<T> RollbackTrigger<RollbackTriggerResult<T>, release::Error> for Result<T, transaction::Error>
where
    T: Send,
{
    async fn on_err_rollback(
        self,
        client: Client,
        plan: &rollback::Plan,
    ) -> Result<RollbackTriggerResult<T>, release::Error> {
        match self {
            Ok(result) => Ok(RollbackTriggerResult {
                result,
                rollback_client: client,
            }),

            Err(cause) => {
                let rollback_result = plan.execute(client).await;
                Err(match rollback_result {
                    Ok(_) => release::Error::ReleaseError { error: cause },
                    Err(error) => release::Error::RollbackError { error, cause },
                })
            }
        }
    }
}
