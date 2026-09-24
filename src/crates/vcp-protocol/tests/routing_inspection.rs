// SPDX-License-Identifier: Apache-2.0
use vcp_protocol::{
    methods::{Id, Scope},
    routing_inspection::{self as wire, Request, Section},
};
#[test]
fn routing_status_bounds_and_strict_fields() {
    let id = || Id::try_from("fixture".to_owned()).unwrap();
    let mut request = Request {
        scope: Scope {
            workspace: id(),
            session: id(),
        },
        task: id(),
        section: Section::Catalog,
        limit: 32,
        cursor: None,
    };
    assert!(wire::validate_request(&request).is_ok());
    request.limit = 33;
    assert!(wire::validate_request(&request).is_err());
    request.limit = 1;
    request.cursor = Some("x".repeat(4097));
    assert!(wire::validate_request(&request).is_err());
    request.cursor = None;
    let mut value = serde_json::to_value(request).unwrap();
    value["credentials"] = serde_json::json!({});
    assert!(serde_json::from_value::<Request>(value).is_err());
}
