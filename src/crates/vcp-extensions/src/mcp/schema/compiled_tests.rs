// SPDX-License-Identifier: Apache-2.0
use super::super::exact::{parse, Budget, Error, Limits};
use super::Schema;
use serde_json::{json, Value};

fn schema(value: &str) -> Schema {
    Schema::compile(value.as_bytes(), Limits::default()).unwrap()
}
fn accepted(schema: &Schema, value: &str) -> bool {
    match schema.arguments(value.as_bytes()) {
        Ok(_) => true,
        Err(Error::Arguments) => false,
        Err(e) => panic!("unexpected {e:?}"),
    }
}
fn rejected_schema(value: &str) {
    assert!(
        Schema::compile(value.as_bytes(), Limits::default()).is_err(),
        "unsupported schema admitted"
    );
}

#[test]
fn roots_remain_explicit_objects_and_nested_booleans_are_real_schemas() {
    for value in [
        "true",
        "false",
        "{}",
        r#"{"type":"array"}"#,
        r#"{"type":["object","null"]}"#,
    ] {
        rejected_schema(value);
    }
    let s = schema(
        r#"{"type":"object","properties":{"anything":true,"never":false},"additionalProperties":false}"#,
    );
    assert!(accepted(&s, r#"{"anything":[1.2,null,{"x":true}]}"#));
    for input in ["null", "1", "[]", r#"{"never":null}"#, r#"{"extra":1}"#] {
        assert!(!accepted(&s, input));
    }
}

#[test]
fn nullable_required_missing_and_schema_dictionaries_are_distinct() {
    let s = schema(
        r#"{"type":"object","properties":{"v":{"type":["integer","null"]}},"required":["v"],"additionalProperties":{"type":"string","minLength":1}}"#,
    );
    for value in [r#"{"v":null}"#, r#"{"v":1.0,"label":"💚"}"#, r#"{"v":1e2}"#] {
        assert!(accepted(&s, value));
    }
    for value in [
        "{}",
        r#"{"v":"1"}"#,
        r#"{"v":1.1}"#,
        r#"{"v":1,"extra":2}"#,
        r#"{"v":1,"extra":""}"#,
    ] {
        assert!(!accepted(&s, value));
    }
    for ty in [
        r#"["string","boolean"]"#,
        r#"["null","null"]"#,
        r#"["string","null","integer"]"#,
        r#"["array","null"]"#,
        r#"[]"#,
    ] {
        rejected_schema(&format!(
            r#"{{"type":"object","properties":{{"v":{{"type":{ty}}}}}}}"#
        ));
    }
    let unspecified = schema(r#"{"type":"object","required":["unknown"]}"#);
    assert!(accepted(&unspecified, r#"{"unknown":1.2}"#));
    assert!(!accepted(&unspecified, "{}"));
}

#[test]
fn exact_numeric_predicates_and_applicability_use_no_epsilon() {
    let s = schema(
        r#"{"type":"object","properties":{"n":{"type":"number","minimum":0.1,"maximum":1,"exclusiveMinimum":0.2,"exclusiveMaximum":0.9,"multipleOf":0.1}},"required":["n"]}"#,
    );
    for value in ["0.3", "3e-1", "0.8"] {
        assert!(accepted(&s, &format!("{{\"n\":{value}}}")));
    }
    for value in [
        "0.1",
        "0.2",
        "0.9",
        "1.1",
        "0.30000000000000000000001",
        "null",
    ] {
        assert!(!accepted(&s, &format!("{{\"n\":{value}}}")));
    }
    let applicability =
        schema(r#"{"type":"object","properties":{"n":{"minimum":10,"minLength":2}}}"#);
    assert!(accepted(&applicability, r#"{"n":"text"}"#));
    assert!(accepted(&applicability, r#"{"n":10.0}"#));
    assert!(accepted(&applicability, r#"{"n":null}"#));
    assert!(!accepted(&applicability, r#"{"n":"x"}"#));
    assert!(!accepted(
        &applicability,
        r#"{"n":9.999999999999999999999}"#
    ));
    for predicate in [
        r#""multipleOf":0"#,
        r#""multipleOf":-0.1"#,
        r#""minimum":"0.1""#,
        r#""exclusiveMinimum":true"#,
    ] {
        rejected_schema(&format!(
            r#"{{"type":"object","properties":{{"n":{{{predicate}}}}}}}"#
        ));
    }
}

#[test]
fn recursive_enum_const_and_uniqueness_use_exact_equality() {
    let s = schema(
        r#"{"type":"object","properties":{"v":{"enum":[1,{"a":[0.3,true]}]},"fixed":{"const":{"x":9007199254740993.1}},"rows":{"type":"array","uniqueItems":true}},"required":["v"]}"#,
    );
    assert!(accepted(
        &s,
        r#"{"v":1.0,"fixed":{"x":90071992547409931e-1},"rows":[9007199254740992,9007199254740993]}"#
    ));
    assert!(accepted(&s, r#"{"v":{"a":[3e-1,true]}}"#));
    for value in [
        r#"{"v":1.1}"#,
        r#"{"v":{"a":[true,0.3]}}"#,
        r#"{"v":1,"fixed":{"x":9007199254740993.2}}"#,
        r#"{"v":1,"rows":[{"a":0.3,"b":true},{"b":true,"a":3e-1}]}"#,
        r#"{"v":1,"rows":[0,-0.0]}"#,
    ] {
        assert!(!accepted(&s, value));
    }
    for enumeration in [
        "[1,1.0]",
        "[0,-0]",
        r#"[{"a":1,"b":2},{"b":2e0,"a":1.0}]"#,
        "[]",
    ] {
        rejected_schema(&format!(
            r#"{{"type":"object","properties":{{"v":{{"enum":{enumeration}}}}}}}"#
        ));
    }
}

#[test]
fn arrays_keep_uniform_items_and_boolean_item_semantics() {
    let s = schema(
        r#"{"type":"object","properties":{"a":{"type":"array","items":{"type":"integer"},"minItems":1,"maxItems":2},"empty":{"type":"array","items":false}}}"#,
    );
    assert!(accepted(&s, r#"{"a":[1.0,2e0],"empty":[]}"#));
    for value in [
        r#"{"a":[]}"#,
        r#"{"a":[1,2,3]}"#,
        r#"{"a":[1.1]}"#,
        r#"{"empty":[null]}"#,
    ] {
        assert!(!accepted(&s, value));
    }
}

#[test]
fn refs_are_local_acyclic_and_siblings_remain_conjunctive() {
    let s = schema(
        r##"{"type":"object","$defs":{"a/b~c":{"type":"number","minimum":0.1},"forward":{"$ref":"#/$defs/a~1b~0c"}},"properties":{"v":{"$ref":"#/$defs/forward","maximum":0.3}},"required":["v"]}"##,
    );
    assert!(accepted(&s, r#"{"v":0.2}"#));
    assert!(!accepted(&s, r#"{"v":0.4}"#));
    assert!(!accepted(&s, r#"{"v":0}"#));
    for value in [
        r##"{"type":"object","$defs":{"x":{"$ref":"#/$defs/x"}}}"##,
        r##"{"type":"object","$defs":{"x":{"$ref":"#/$defs/y"},"y":{"$ref":"#/$defs/x"}}}"##,
        r##"{"type":"object","$ref":"#/$defs/missing"}"##,
        r##"{"type":"object","$ref":"https://example.test/schema"}"##,
        r##"{"type":"object","$ref":"file:///secret"}"##,
        r##"{"type":"object","$defs":{"a/b":true},"$ref":"#/$defs/a/b"}"##,
        r##"{"type":"object","$defs":{"a/b":true},"$ref":"#/$defs/a%2Fb"}"##,
        r##"{"type":"object","$defs":{"a":true},"$ref":"#/$defs/a~2"}"##,
        r#"{"type":"object","properties":{"v":{"$defs":{"x":true}}}}"#,
    ] {
        rejected_schema(value);
    }
}

#[test]
fn local_ref_uri_profile_rejects_invalid_fragments_even_with_matching_keys() {
    // Percent decoding would target a/b, whose schema is false. Literal matching
    // would incorrectly select a%2Fb and accept. This profile instead rejects.
    rejected_schema(
        r##"{"type":"object","$defs":{"a%2Fb":true,"a/b":false},"$ref":"#/$defs/a%2Fb"}"##,
    );
    for name in [
        "a%2Fb", "a%ZZ", "a b", "a\tb", "a\nb", "a\rb", "a\\b", "a💚b", "a#b", "a[b]", "a{b}",
        "a\"b", "a|b", "a^b", "a`b", "a\0b",
    ] {
        let mut definitions = serde_json::Map::new();
        definitions.insert(name.into(), json!(true));
        let source = json!({"type":"object","$defs":definitions,"$ref":format!("#/$defs/{name}")});
        assert_eq!(
            Schema::compile(&serde_json::to_vec(&source).unwrap(), Limits::default()).unwrap_err(),
            Error::Schema,
            "invalid URI fragment was admitted",
        );
    }
    for name in ["", "a-._!$&'()*+,;=:@?9"] {
        let mut definitions = serde_json::Map::new();
        definitions.insert(name.into(), json!(true));
        let source = json!({"type":"object","$defs":definitions,"$ref":format!("#/$defs/{name}")});
        assert!(accepted(&schema(&source.to_string()), "{}"));
    }
}

#[test]
fn additional_properties_do_not_merge_across_refs_or_all_of() {
    let reference = schema(
        r##"{"type":"object","$defs":{"base":{"properties":{"a":true}}},"$ref":"#/$defs/base","properties":{"b":true},"additionalProperties":false}"##,
    );
    let composition = schema(
        r#"{"type":"object","allOf":[{"properties":{"a":true}}],"properties":{"b":true},"additionalProperties":false}"#,
    );
    for s in [&reference, &composition] {
        assert!(accepted(s, r#"{"b":2}"#));
        assert!(!accepted(s, r#"{"a":1,"b":2}"#));
    }
    let closed = schema(
        r#"{"type":"object","allOf":[{"properties":{"a":true},"additionalProperties":false},{"properties":{"b":true},"additionalProperties":false}]}"#,
    );
    assert!(accepted(&closed, "{}"));
    assert!(!accepted(&closed, r#"{"a":1}"#));
    assert!(!accepted(&closed, r#"{"b":2}"#));
}

#[test]
fn compositions_are_source_ordered_and_exactly_one_means_exactly_one() {
    let s = schema(
        r#"{"type":"object","properties":{"x":{"allOf":[{"type":"number"},{"minimum":0}],"anyOf":[{"maximum":1},{"minimum":10}],"not":{"const":0}},"y":{"oneOf":[{"type":"integer"},{"type":"number","minimum":0}]}}}"#,
    );
    assert!(accepted(&s, r#"{"x":0.5,"y":-1}"#));
    assert!(accepted(&s, r#"{"x":10,"y":0.5}"#));
    for value in [r#"{"x":0}"#, r#"{"x":2}"#, r#"{"y":1}"#, r#"{"y":null}"#] {
        assert!(!accepted(&s, value));
    }
    // Later branches are compiled even when an earlier boolean would short-circuit.
    rejected_schema(r#"{"type":"object","anyOf":[true,{"format":"email"}]}"#);
}

fn expanding_schema(wrapper: &str) -> Value {
    let mut defs = serde_json::Map::new();
    defs.insert("d0".into(), json!(true));
    for index in 1..8 {
        let target = format!("#/$defs/d{}", index - 1);
        defs.insert(
            format!("d{index}"),
            json!({"allOf":[{"$ref":target},{"$ref":target}]}),
        );
    }
    let branch = json!({"$ref":"#/$defs/d7"});
    let mut result = json!({"type":"object","$defs":defs});
    result[wrapper] = match wrapper {
        "not" => branch,
        "anyOf" => json!([branch, true]),
        _ => json!([branch, false]),
    };
    result
}

#[test]
fn shared_fuel_errors_are_not_negated_or_swallowed_by_alternatives() {
    let parsed = parse(b"{}", Limits::default()).unwrap();
    for wrapper in ["not", "anyOf", "oneOf"] {
        let s = schema(&expanding_schema(wrapper).to_string());
        assert_eq!(
            s.validates_parsed(parsed.value(), &mut Budget::new(20, 1000)),
            Err(Error::Bound)
        );
        assert_eq!(accepted(&s, "{}"), wrapper != "not");
    }
    let s = schema(r#"{"type":"object","properties":{"a":{"uniqueItems":true}}}"#);
    let input = parse(
        br#"{"a":[{"x":"large shared prefix a"},{"x":"large shared prefix b"}]}"#,
        Limits::default(),
    )
    .unwrap();
    assert_eq!(
        s.validates_parsed(input.value(), &mut Budget::new(10, 100)),
        Err(Error::Bound)
    );
}

#[test]
fn every_definition_and_keyword_is_admitted_even_when_unreachable() {
    for keyword in [
        "pattern",
        "format",
        "patternProperties",
        "propertyNames",
        "dependentRequired",
        "unevaluatedProperties",
        "prefixItems",
        "contains",
        "if",
        "$id",
        "$anchor",
        "$dynamicRef",
        "$vocabulary",
    ] {
        let input = json!({"type":"object","$defs":{"unused":{keyword:true}}});
        rejected_schema(&input.to_string());
    }
    for input in [
        r#"{"type":"object","required":["a","a"]}"#,
        r#"{"type":"object","properties":{"a":{"type":"string","minLength":-1}}}"#,
        r#"{"type":"object","properties":{"a":{"type":"array","maxItems":1.5}}}"#,
        r#"{"type":"object","properties":{"a":{"uniqueItems":"true"}}}"#,
        r#"{"type":"object","allOf":[]}"#,
        r#"{"type":"object","not":1}"#,
        r#"{"type":"object","$schema":"https://example.test/other"}"#,
    ] {
        rejected_schema(input);
    }
    let s = schema(
        r#"{"type":"object","default":{"format":"inert"},"examples":[{"pattern":"inert"}],"title":"private-sentinel","description":"metadata","$comment":"inert"}"#,
    );
    assert!(accepted(&s, "{}"));
    assert!(!format!("{s:?}").contains("private-sentinel"));
}

#[test]
fn arena_reference_and_branch_limits_are_checked_without_expansion() {
    let broad = json!({"type":"object","allOf":vec![json!(true);17]});
    assert_eq!(
        Schema::compile(&serde_json::to_vec(&broad).unwrap(), Limits::default()).unwrap_err(),
        Error::Bound
    );
    let mut defs = serde_json::Map::new();
    defs.insert("base".into(), json!(true));
    let props: serde_json::Map<_, _> = (0..128)
        .map(|i| (format!("p{i}"), json!({"$ref":"#/$defs/base"})))
        .collect();
    for index in 0..3 {
        defs.insert(format!("d{index}"), json!({"properties":props}));
    }
    let refs = json!({"type":"object","$defs":defs});
    assert_eq!(
        Schema::compile(&serde_json::to_vec(&refs).unwrap(), Limits::default()).unwrap_err(),
        Error::Bound
    );
    let mut defs = serde_json::Map::new();
    let props: serde_json::Map<_, _> = (0..128).map(|i| (format!("p{i}"), json!(true))).collect();
    for index in 0..8 {
        defs.insert(format!("d{index}"), json!({"properties":props}));
    }
    let nodes = json!({"type":"object","$defs":defs});
    assert_eq!(
        Schema::compile(
            &serde_json::to_vec(&nodes).unwrap(),
            Limits {
                nodes: 65536,
                ..Limits::default()
            }
        )
        .unwrap_err(),
        Error::Bound
    );
    let mut defs = serde_json::Map::new();
    defs.insert("d0".into(), json!(true));
    for index in 1..70 {
        defs.insert(
            format!("d{index}"),
            json!({"$ref":format!("#/$defs/d{}",index-1)}),
        );
    }
    let deep = json!({"type":"object","$defs":defs});
    assert_eq!(
        Schema::compile(&serde_json::to_vec(&deep).unwrap(), Limits::default()).unwrap_err(),
        Error::Bound
    );
}

#[test]
fn schema_and_argument_canonical_values_remain_exact_and_deterministic() {
    let a = schema(r#"{"type":"object","properties":{"n":{"minimum":0.3,"type":"number"}}}"#);
    let b = schema(r#"{"properties":{"n":{"type":"number","minimum":3e-1}},"type":"object"}"#);
    assert_eq!(a.canonical_bytes(), b.canonical_bytes());
    assert_eq!(
        a.arguments(br#"{"n":9007199254740993.1}"#).unwrap(),
        br#"{"n":90071992547409931e-1}"#
    );
}
