use crate::k8s;
use crate::k8s::api_resource;
use crate::k8s::labels;
use crate::release;
use serde_json::value::Value;
use std::collections::BTreeMap;
use std::collections::VecDeque;

pub async fn find_release_objects(
    mut client: kube::Client,
    release_name: String,
) -> Result<release::Objects, kube::Error> {
    let all_resources = api_resource::find_api_resources(&client).await?;
    let labels = labels::Labels::from(k8s::ObjectType::Managed)
        .add(k8s::ReleaseName(release_name))
        .to_listparams();

    let mut all_items = BTreeMap::new();

    for resource in all_resources {
        let api: kube::Api<kube::core::DynamicObject> = kube::Api::all_with(client, &resource);
        let items = api.list(&labels).await?.items;

        all_items.extend(
            items
                .into_iter()
                .filter_map(|item| item.metadata.name.clone().map(|name| (name, item))),
        );

        client = api.into_client();
    }

    Ok(all_items)
}

pub fn check_value(
    spec: &serde_json::Value,
    instance: &serde_json::Value,
    path: VecDeque<String>,
) -> Result<(), VecDeque<String>> {
    match (spec, instance) {
        (Value::Null, Value::Null) => {}

        (Value::Bool(spec), Value::Bool(i)) if spec == i => {}

        (Value::Number(spec), Value::Number(i)) if spec == i => {}

        (Value::String(spec), Value::String(i)) if spec == i => {}

        (Value::Array(spec), Value::Array(i)) if spec.len() == i.len() => {
            for index in 0..i.len() {
                let mut path = path.clone();
                path.push_back(format!("{}", index));
                check_value(&spec[index], &i[index], path)?;
            }
        }

        (Value::Object(spec), Value::Object(instance)) => {
            for (key, spec_value) in spec {
                let cloned_path = || {
                    let mut path = path.clone();
                    path.push_back(key.clone());
                    path
                };

                instance.get(key).map_or_else(
                    || Err(cloned_path()),
                    |instance_value| check_value(spec_value, instance_value, cloned_path()),
                )?;
            }
        }

        _ => return Err(path),
    }

    Ok(())
}

pub fn check_mapping(spec: &BTreeMap<String, String>, instance: &BTreeMap<String, String>) -> bool {
    spec.iter().all(|(key, spec_value)| {
        instance
            .get(key)
            .map_or(false, |instance_value| instance_value == spec_value)
    })
}
