// SPDX-License-Identifier: Apache-2.0
//! Qualification evidence joins, separate from raw Responses identities.
use crate::{stream::ResultBody, Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attribution {
    pub response_id: String,
    pub requested_model: String,
    pub observed_model_revision: String,
    pub catalog_endpoint: String,
    pub provider_name: String,
    pub observed_endpoint_id: String,
    pub generation_sha256: String,
    pub method: String,
}

fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 256 && !s.chars().any(char::is_control))
        .ok_or(Error::Protocol("bounded generation identity"))
}

/// A provider display name is usable only when the complete dated model catalog
/// maps it to exactly one endpoint. Regional/variant ambiguity rejects. The UUID
/// remains an observed internal identity, never represented as a catalog field.
pub fn from_generation(
    raw_catalog: &[u8],
    raw_generation: &[u8],
    response: &ResultBody,
    model: &str,
    endpoint: &str,
) -> Result<Attribution> {
    if raw_catalog.len() > 4 * 1024 * 1024 || raw_generation.len() > 1024 * 1024 {
        return Err(Error::Limit("attribution bytes"));
    }
    if response.status != crate::stream::Status::Completed
        || response.served_model.as_deref() != Some(model)
    {
        return Err(Error::Protocol("attribution completed requested model"));
    }
    let catalog: Value = serde_json::from_slice(raw_catalog)?;
    let generation: Value = serde_json::from_slice(raw_generation)?;
    let data = &generation["data"];
    if catalog["data"]["id"] != model
        || text(data, "id")? != response.response_id
        || data["cancelled"] != false
        || data["streamed"] != true
        || data["is_byok"] != false
    {
        return Err(Error::Protocol("generation request binding"));
    }
    let provider = text(data, "provider_name")?;
    let revision = text(data, "model")?;
    let endpoints = catalog["data"]["endpoints"]
        .as_array()
        .ok_or(Error::Protocol("attribution catalog endpoints"))?;
    let named: Vec<_> = endpoints
        .iter()
        .filter(|e| e["provider_name"] == provider)
        .collect();
    if named.len() != 1
        || named[0]["tag"] != endpoint
        || named[0]["status"] != 0
        || named[0]["model_id"] != model
        || text(named[0], "name")? != format!("{provider} | {revision}")
        || endpoints.iter().any(|e| {
            e["tag"].as_str().is_some_and(|tag| {
                tag.strip_prefix(endpoint)
                    .is_some_and(|suffix| suffix.starts_with('/'))
            })
        })
    {
        return Err(Error::Capability(
            "ambiguous or mismatched served model/provider catalog mapping",
        ));
    }
    let attempts = data["provider_responses"]
        .as_array()
        .ok_or(Error::Protocol("generation provider attempts"))?;
    if attempts.len() != 1
        || attempts[0]["status"] != 200
        || attempts[0]["provider_name"] != provider
        || attempts[0]["model_permaslug"] != revision
        || attempts[0]["is_byok"] != false
    {
        return Err(Error::Protocol("generation single served provider attempt"));
    }
    if response
        .served_provider
        .as_deref()
        .is_some_and(|p| p != endpoint && p != provider)
    {
        return Err(Error::Protocol("conflicting Responses provider identity"));
    }
    let cost = data["total_cost"]
        .as_number()
        .ok_or(Error::Protocol("generation cost missing"))?
        .to_string();
    let observed = response
        .usage
        .as_ref()
        .and_then(|u| u.cost.as_ref())
        .ok_or(Error::Protocol("response cost missing"))?;
    if observed.currency.code() != "USD"
        || crate::catalog::usd_micros(&cost)? != observed.micros.get()
    {
        return Err(Error::Protocol("generation charge differs from response"));
    }
    Ok(Attribution {
        response_id: response.response_id.clone(),
        requested_model: model.into(),
        observed_model_revision: revision.into(),
        catalog_endpoint: endpoint.into(),
        provider_name: provider.into(),
        observed_endpoint_id: text(&attempts[0], "endpoint_id")?.into(),
        generation_sha256: vcp_protocol::digest_bytes(raw_generation),
        method: "generation-single-attempt-exact-catalog-model-provider/2".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn attribution_requires_unambiguous_current_catalog_and_matching_receipt() {
        let catalog = json!({"data":{"id":"fixture/alias","endpoints":[{"tag":"fixture","provider_name":"Fixture Provider","model_id":"fixture/alias","name":"Fixture Provider | fixture/revision-1","status":0}]}});
        let generation = json!({"data":{"id":"gen-1","model":"fixture/revision-1","provider_name":"Fixture Provider","cancelled":false,"streamed":true,"is_byok":false,"total_cost":0.000002,"provider_responses":[{"endpoint_id":"internal-id","provider_name":"Fixture Provider","model_permaslug":"fixture/revision-1","status":200,"is_byok":false}]}});
        let response:ResultBody=serde_json::from_value(json!({"response_id":"gen-1","served_model":"fixture/alias","served_provider":null,"status":"Completed","usage":{"raw":{},"tokens":null,"cost":{"currency":"USD","micros":"2"}},"calls":[],"raw_terminal_sha256":"raw"})).unwrap();
        let check = |c: &Value, g: &Value, r: &ResultBody| {
            from_generation(
                &serde_json::to_vec(c).unwrap(),
                &serde_json::to_vec(g).unwrap(),
                r,
                "fixture/alias",
                "fixture",
            )
        };
        let evidence = check(&catalog, &generation, &response).unwrap();
        assert_eq!(evidence.observed_model_revision, "fixture/revision-1");
        assert!(response.served_provider.is_none());
        let mut ambiguous = catalog.clone();
        ambiguous["data"]["endpoints"]
            .as_array_mut()
            .unwrap()
            .push(json!({"tag":"fixture/region","provider_name":"Fixture Provider","status":0}));
        assert!(check(&ambiguous, &generation, &response).is_err());
        for field in ["id", "model", "provider_name", "total_cost", "cancelled"] {
            let mut g = generation.clone();
            g["data"][field] = json!("wrong");
            assert!(check(&catalog, &g, &response).is_err(), "{field}");
        }
        let mut g = generation.clone();
        let attempt = g["data"]["provider_responses"][0].clone();
        g["data"]["provider_responses"]
            .as_array_mut()
            .unwrap()
            .push(attempt);
        assert!(check(&catalog, &g, &response).is_err());
        let mut r = response.clone();
        r.served_provider = Some("other".into());
        assert!(check(&catalog, &generation, &r).is_err());
        let mut wrong_model = generation.clone();
        wrong_model["data"]["model"] = json!("other/model-revision");
        wrong_model["data"]["provider_responses"][0]["model_permaslug"] =
            json!("other/model-revision");
        assert!(check(&catalog, &wrong_model, &response).is_err());
        for field in ["model_id", "name"] {
            let mut c = catalog.clone();
            c["data"]["endpoints"][0][field] = json!("wrong");
            assert!(check(&c, &generation, &response).is_err(), "{field}");
            c["data"]["endpoints"][0]
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                check(&c, &generation, &response).is_err(),
                "missing {field}"
            );
        }
    }
}
