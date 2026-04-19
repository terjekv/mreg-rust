//! Typed envelopes for the `data` field of [`RecordResponse`].
//!
//! The HTTP record API serves a unified `RecordResponse` whose `data` shape
//! varies by `type_name`. Historically `data` was an unstructured JSON `Value`
//! and consumers had to know the per-type schema themselves. The types in this
//! module project the wire shape into one Rust struct per built-in DNS record
//! type, with an open-set fallback for runtime-registered types.
//!
//! ## Wire shape
//!
//! The published OpenAPI schema for [`RecordKind`] is an unconstrained
//! `oneOf` of [`TypedRecordKind`] (25 built-in variants, each carrying a
//! concrete `data` struct) and [`OpaqueRecordKind`] (catch-all for unknown
//! `type_name` values). The `type_name` field acts as the *effective*
//! dispatch tag for the built-ins:
//!
//! - if `type_name` matches a built-in, [`TypedRecordKind`] deserializes
//!   `data` against that variant's schema; consumers get fully-typed access.
//! - otherwise the response falls into [`RecordKind::Opaque`], which preserves
//!   the dynamic `type_name` string and leaves `data` as either `null`
//!   (RFC 3597 raw RDATA records) or an opaque JSON object (operator-defined
//!   types registered via `POST /api/v1/dns/record-types`).
//!
//! ## OpenAPI / client-generation caveat
//!
//! This is **not** an OpenAPI `discriminator`-tagged union. utoipa cannot
//! emit a discriminator over an untagged wrapper, and the typed half is
//! intentionally open-set. Generated clients should switch on `type_name`
//! manually and fall back to the opaque variant on unknown values rather
//! than relying on tagged-union code generation.
//!
//! Drift between this enum and [`crate::domain::builtin_types`] is enforced by
//! the rstest suite at the bottom of this file.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::ToSchema;

use crate::domain::resource_records::RecordInstance;

/// Polymorphic record payload.  Either matches one of the 25 known built-in
/// types (variant `Typed`) or carries an arbitrary user-registered type name
/// plus opaque JSON / `null` data (variant `Opaque`).
///
/// When this enum is `#[serde(flatten)]`-ed into [`RecordResponse`], the
/// `Typed` variant contributes `"type_name": "<TYPE>"` and `"data": { ... }`;
/// the `Opaque` variant contributes `"type_name": "<custom>"` and
/// `"data": null` (or whatever JSON the storage layer round-tripped).
///
/// The published OpenAPI schema is `oneOf` without a discriminator object —
/// see the module-level docs for why and what consumers should do instead.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(untagged)]
pub enum RecordKind {
    Typed(TypedRecordKind),
    Opaque(OpaqueRecordKind),
}

impl RecordKind {
    /// Project a stored record instance into the public polymorphic response
    /// shape. Built-ins deserialize into [`TypedRecordKind`]; runtime-defined
    /// types and malformed stored payloads fall back to [`OpaqueRecordKind`].
    pub(crate) fn from_record(record: &RecordInstance) -> Self {
        let type_name = record.type_name().as_str();
        let opaque_data = if record.raw_rdata().is_some() || record.data().is_null() {
            None
        } else {
            Some(record.data().clone())
        };
        let opaque = || {
            Self::Opaque(OpaqueRecordKind {
                type_name: type_name.to_string(),
                data: opaque_data.clone(),
            })
        };

        if record.data().is_null() {
            return opaque();
        }

        let envelope = serde_json::json!({ "type_name": type_name, "data": record.data() });
        match serde_json::from_value::<TypedRecordKind>(envelope) {
            Ok(typed) => Self::Typed(typed),
            Err(error) => {
                tracing::warn!(
                    %type_name,
                    %error,
                    "stored record data did not match typed schema; serving as opaque",
                );
                opaque()
            }
        }
    }
}

/// `oneOf` of all 25 built-in DNS record types.  Externally tagged on
/// `type_name` with the payload under `data` at the serde layer; the
/// published OpenAPI schema is `oneOf` without a `discriminator` object
/// (utoipa 5.x does not support emitting a discriminator for this enum
/// shape — see the module-level docs).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
#[serde(tag = "type_name", content = "data")]
pub enum TypedRecordKind {
    A(ARecordData),
    AAAA(AaaaRecordData),
    NS(NsRecordData),
    PTR(PtrRecordData),
    CNAME(CnameRecordData),
    DNAME(DnameRecordData),
    MX(MxRecordData),
    TXT(TxtRecordData),
    SRV(SrvRecordData),
    NAPTR(NaptrRecordData),
    SSHFP(SshfpRecordData),
    LOC(LocRecordData),
    HINFO(HinfoRecordData),
    DS(DsRecordData),
    DNSKEY(DnskeyRecordData),
    CDS(CdsRecordData),
    CDNSKEY(CdnskeyRecordData),
    CSYNC(CsyncRecordData),
    CAA(CaaRecordData),
    TLSA(TlsaRecordData),
    SMIMEA(SmimeaRecordData),
    SVCB(SvcbRecordData),
    HTTPS(HttpsRecordData),
    URI(UriRecordData),
    OPENPGPKEY(OpenpgpkeyRecordData),
}

/// Fallback for record types that aren't built-in.  `type_name` is whatever
/// the operator registered (e.g. `"TYPE65400"`), and `data` is either the
/// validated JSON for the type's user-defined schema or `null` for RFC 3597
/// opaque records (in which case `raw_rdata` carries the wire bytes).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct OpaqueRecordKind {
    pub type_name: String,
    /// `null` for RFC 3597 raw-rdata records; otherwise an opaque JSON object
    /// shaped per the operator-registered field schema. Must be declared
    /// nullable because `RecordKind::from_record` emits `data: null` whenever the
    /// underlying record carries no structured payload.
    #[serde(default)]
    #[schema(value_type = Option<Object>, nullable = true)]
    pub data: Option<Value>,
}

// ---------------------------------------------------------------------------
// Per-type data structs.  Field names and types must match the runtime field
// schemas in `src/domain/builtin_types/*.rs`; the drift test below enforces
// that.
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ARecordData {
    pub address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct AaaaRecordData {
    pub address: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct NsRecordData {
    pub nsdname: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct PtrRecordData {
    pub ptrdname: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CnameRecordData {
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct DnameRecordData {
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct MxRecordData {
    pub preference: u16,
    pub exchange: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TxtRecordData {
    /// One or more presentation-form character-strings (RFC 1035 §3.3.14).
    pub value: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SrvRecordData {
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct NaptrRecordData {
    pub order: u16,
    pub preference: u16,
    pub flags: String,
    pub services: String,
    pub regexp: String,
    pub replacement: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SshfpRecordData {
    pub algorithm: u16,
    pub fp_type: u16,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct LocRecordData {
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub horizontal_precision_m: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vertical_precision_m: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct HinfoRecordData {
    pub cpu: String,
    pub os: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct DsRecordData {
    pub key_tag: u16,
    pub algorithm: u16,
    pub digest_type: u16,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct DnskeyRecordData {
    pub flags: u16,
    pub protocol: u16,
    pub algorithm: u16,
    pub public_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CdsRecordData {
    pub key_tag: u16,
    pub algorithm: u16,
    pub digest_type: u16,
    pub digest: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CdnskeyRecordData {
    pub flags: u16,
    pub protocol: u16,
    pub algorithm: u16,
    pub public_key: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CsyncRecordData {
    pub soa_serial: u32,
    pub flags: u16,
    pub type_bitmap: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CaaRecordData {
    pub flags: u16,
    pub tag: String,
    pub value: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct TlsaRecordData {
    pub usage: u16,
    pub selector: u16,
    pub matching_type: u16,
    pub certificate_data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SmimeaRecordData {
    pub usage: u16,
    pub selector: u16,
    pub matching_type: u16,
    pub certificate_data: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct SvcbRecordData {
    pub priority: u16,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct HttpsRecordData {
    pub priority: u16,
    pub target: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct UriRecordData {
    pub priority: u16,
    pub weight: u16,
    pub target: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct OpenpgpkeyRecordData {
    pub public_key: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::builtin_types::built_in_record_types;
    use rstest::rstest;
    use serde_json::json;
    use std::collections::HashSet;

    struct TypedSchemaManifestEntry {
        type_name: &'static str,
        schema_name: &'static str,
    }

    const TYPED_SCHEMA_MANIFEST: &[TypedSchemaManifestEntry] = &[
        TypedSchemaManifestEntry {
            type_name: "A",
            schema_name: "ARecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "AAAA",
            schema_name: "AaaaRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "NS",
            schema_name: "NsRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "PTR",
            schema_name: "PtrRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "CNAME",
            schema_name: "CnameRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "DNAME",
            schema_name: "DnameRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "MX",
            schema_name: "MxRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "TXT",
            schema_name: "TxtRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "SRV",
            schema_name: "SrvRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "NAPTR",
            schema_name: "NaptrRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "SSHFP",
            schema_name: "SshfpRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "LOC",
            schema_name: "LocRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "HINFO",
            schema_name: "HinfoRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "DS",
            schema_name: "DsRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "DNSKEY",
            schema_name: "DnskeyRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "CDS",
            schema_name: "CdsRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "CDNSKEY",
            schema_name: "CdnskeyRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "CSYNC",
            schema_name: "CsyncRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "CAA",
            schema_name: "CaaRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "TLSA",
            schema_name: "TlsaRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "SMIMEA",
            schema_name: "SmimeaRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "SVCB",
            schema_name: "SvcbRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "HTTPS",
            schema_name: "HttpsRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "URI",
            schema_name: "UriRecordData",
        },
        TypedSchemaManifestEntry {
            type_name: "OPENPGPKEY",
            schema_name: "OpenpgpkeyRecordData",
        },
    ];

    fn envelope(type_name: &str, data: Value) -> Value {
        json!({ "type_name": type_name, "data": data })
    }

    /// Asserts that the typed variant round-trips through the canonical envelope
    /// in both directions: serialize-as-envelope and deserialize-as-variant.
    fn assert_typed_round_trip(type_name: &str, data: Value, variant: TypedRecordKind) {
        let canonical = envelope(type_name, data);

        let serialized = serde_json::to_value(&variant)
            .unwrap_or_else(|err| panic!("serialize {type_name}: {err}"));
        assert_eq!(
            serialized, canonical,
            "typed variant for {type_name} did not serialize to canonical envelope",
        );

        let deserialized: TypedRecordKind = serde_json::from_value(canonical)
            .unwrap_or_else(|err| panic!("deserialize {type_name}: {err}"));
        assert_eq!(
            deserialized, variant,
            "canonical envelope for {type_name} did not deserialize to expected variant",
        );
    }

    // ---- Per-built-in canonical round-trips. One case per built-in type. ----

    #[rstest]
    #[case::a("A",
        json!({"address": "192.0.2.1"}),
        TypedRecordKind::A(ARecordData { address: "192.0.2.1".into() }))]
    #[case::aaaa("AAAA",
        json!({"address": "2001:db8::1"}),
        TypedRecordKind::AAAA(AaaaRecordData { address: "2001:db8::1".into() }))]
    #[case::ns("NS",
        json!({"nsdname": "ns1.example.org."}),
        TypedRecordKind::NS(NsRecordData { nsdname: "ns1.example.org.".into() }))]
    #[case::ptr("PTR",
        json!({"ptrdname": "host.example.org."}),
        TypedRecordKind::PTR(PtrRecordData { ptrdname: "host.example.org.".into() }))]
    #[case::cname("CNAME",
        json!({"target": "alias.example.org."}),
        TypedRecordKind::CNAME(CnameRecordData { target: "alias.example.org.".into() }))]
    #[case::dname("DNAME",
        json!({"target": "subtree.example.org."}),
        TypedRecordKind::DNAME(DnameRecordData { target: "subtree.example.org.".into() }))]
    #[case::mx("MX",
        json!({"preference": 10, "exchange": "mail.example.org"}),
        TypedRecordKind::MX(MxRecordData { preference: 10, exchange: "mail.example.org".into() }))]
    #[case::txt_single("TXT",
        json!({"value": ["v=spf1 -all"]}),
        TypedRecordKind::TXT(TxtRecordData { value: vec!["v=spf1 -all".into()] }))]
    #[case::srv("SRV",
        json!({"priority": 10, "weight": 60, "port": 5060, "target": "sip.example.org"}),
        TypedRecordKind::SRV(SrvRecordData {
            priority: 10, weight: 60, port: 5060, target: "sip.example.org".into(),
        }))]
    #[case::naptr("NAPTR",
        json!({
            "order": 100, "preference": 10, "flags": "U", "services": "E2U+sip",
            "regexp": "!^.*$!sip:info@example.org!", "replacement": ".",
        }),
        TypedRecordKind::NAPTR(NaptrRecordData {
            order: 100, preference: 10,
            flags: "U".into(), services: "E2U+sip".into(),
            regexp: "!^.*$!sip:info@example.org!".into(), replacement: ".".into(),
        }))]
    #[case::sshfp("SSHFP",
        json!({"algorithm": 1, "fp_type": 1, "fingerprint": "aabbccdd"}),
        TypedRecordKind::SSHFP(SshfpRecordData {
            algorithm: 1, fp_type: 1, fingerprint: "aabbccdd".into(),
        }))]
    #[case::loc_required_only("LOC",
        json!({"latitude": 42.5, "longitude": -71.6, "altitude_m": 100.0}),
        TypedRecordKind::LOC(LocRecordData {
            latitude: 42.5, longitude: -71.6, altitude_m: 100.0,
            size_m: None, horizontal_precision_m: None, vertical_precision_m: None,
        }))]
    #[case::hinfo("HINFO",
        json!({"cpu": "Intel-Xeon", "os": "Linux"}),
        TypedRecordKind::HINFO(HinfoRecordData {
            cpu: "Intel-Xeon".into(), os: "Linux".into(),
        }))]
    #[case::ds("DS",
        json!({"key_tag": 12345, "algorithm": 8, "digest_type": 2, "digest": "abcdef"}),
        TypedRecordKind::DS(DsRecordData {
            key_tag: 12345, algorithm: 8, digest_type: 2, digest: "abcdef".into(),
        }))]
    #[case::dnskey("DNSKEY",
        json!({"flags": 257, "protocol": 3, "algorithm": 8, "public_key": "AwEAAcd"}),
        TypedRecordKind::DNSKEY(DnskeyRecordData {
            flags: 257, protocol: 3, algorithm: 8, public_key: "AwEAAcd".into(),
        }))]
    #[case::cds("CDS",
        json!({"key_tag": 12345, "algorithm": 8, "digest_type": 2, "digest": "abcdef"}),
        TypedRecordKind::CDS(CdsRecordData {
            key_tag: 12345, algorithm: 8, digest_type: 2, digest: "abcdef".into(),
        }))]
    #[case::cdnskey("CDNSKEY",
        json!({"flags": 257, "protocol": 3, "algorithm": 8, "public_key": "AwEAAcd"}),
        TypedRecordKind::CDNSKEY(CdnskeyRecordData {
            flags: 257, protocol: 3, algorithm: 8, public_key: "AwEAAcd".into(),
        }))]
    #[case::csync("CSYNC",
        json!({"soa_serial": 2024010101_u32, "flags": 1, "type_bitmap": "A NS"}),
        TypedRecordKind::CSYNC(CsyncRecordData {
            soa_serial: 2024010101, flags: 1, type_bitmap: "A NS".into(),
        }))]
    #[case::caa("CAA",
        json!({"flags": 0, "tag": "issue", "value": "letsencrypt.org"}),
        TypedRecordKind::CAA(CaaRecordData {
            flags: 0, tag: "issue".into(), value: "letsencrypt.org".into(),
        }))]
    #[case::tlsa("TLSA",
        json!({"usage": 3, "selector": 1, "matching_type": 1, "certificate_data": "deadbeef"}),
        TypedRecordKind::TLSA(TlsaRecordData {
            usage: 3, selector: 1, matching_type: 1, certificate_data: "deadbeef".into(),
        }))]
    #[case::smimea("SMIMEA",
        json!({"usage": 3, "selector": 1, "matching_type": 1, "certificate_data": "deadbeef"}),
        TypedRecordKind::SMIMEA(SmimeaRecordData {
            usage: 3, selector: 1, matching_type: 1, certificate_data: "deadbeef".into(),
        }))]
    #[case::svcb_required_only("SVCB",
        json!({"priority": 1, "target": "svc.example.org"}),
        TypedRecordKind::SVCB(SvcbRecordData {
            priority: 1, target: "svc.example.org".into(), params: None,
        }))]
    #[case::https_required_only("HTTPS",
        json!({"priority": 1, "target": "svc.example.org"}),
        TypedRecordKind::HTTPS(HttpsRecordData {
            priority: 1, target: "svc.example.org".into(), params: None,
        }))]
    #[case::uri("URI",
        json!({"priority": 10, "weight": 1, "target": "https://example.org/"}),
        TypedRecordKind::URI(UriRecordData {
            priority: 10, weight: 1, target: "https://example.org/".into(),
        }))]
    #[case::openpgpkey("OPENPGPKEY",
        json!({"public_key": "mQENBA"}),
        TypedRecordKind::OPENPGPKEY(OpenpgpkeyRecordData { public_key: "mQENBA".into() }))]
    fn canonical_payload_round_trips_through_typed(
        #[case] type_name: &str,
        #[case] data: Value,
        #[case] expected: TypedRecordKind,
    ) {
        assert_typed_round_trip(type_name, data, expected);
    }

    // ---- Optional / repeated field edge cases. ----

    #[test]
    fn loc_optionals_omitted_from_output_when_none() {
        let variant = TypedRecordKind::LOC(LocRecordData {
            latitude: 42.5,
            longitude: -71.6,
            altitude_m: 100.0,
            size_m: None,
            horizontal_precision_m: None,
            vertical_precision_m: None,
        });
        let serialized = serde_json::to_value(&variant).expect("serialize");
        let object = serialized
            .get("data")
            .and_then(Value::as_object)
            .expect("envelope must have data object");
        // None optionals must be omitted, not serialized as null.
        assert!(!object.contains_key("size_m"), "size_m must be omitted");
        assert!(
            !object.contains_key("horizontal_precision_m"),
            "horizontal_precision_m must be omitted",
        );
        assert!(
            !object.contains_key("vertical_precision_m"),
            "vertical_precision_m must be omitted",
        );
    }

    #[rstest]
    #[case::loc_with_all_optionals(
        "LOC",
        json!({
            "latitude": 42.5,
            "longitude": -71.6,
            "altitude_m": 100.0,
            "size_m": 1.0,
            "horizontal_precision_m": 10000.0,
            "vertical_precision_m": 2.0,
        }),
        TypedRecordKind::LOC(LocRecordData {
            latitude: 42.5,
            longitude: -71.6,
            altitude_m: 100.0,
            size_m: Some(1.0),
            horizontal_precision_m: Some(10000.0),
            vertical_precision_m: Some(2.0),
        })
    )]
    #[case::svcb_with_params(
        "SVCB",
        json!({
            "priority": 1,
            "target": "svc.example.org",
            "params": "alpn=h2,h3 port=443",
        }),
        TypedRecordKind::SVCB(SvcbRecordData {
            priority: 1,
            target: "svc.example.org".into(),
            params: Some("alpn=h2,h3 port=443".into()),
        })
    )]
    #[case::https_with_params(
        "HTTPS",
        json!({
            "priority": 1,
            "target": "svc.example.org",
            "params": "alpn=h2,h3",
        }),
        TypedRecordKind::HTTPS(HttpsRecordData {
            priority: 1,
            target: "svc.example.org".into(),
            params: Some("alpn=h2,h3".into()),
        })
    )]
    #[case::txt_with_multiple_values(
        "TXT",
        json!({"value": ["v=spf1", "include:_spf.example.org", "-all"]}),
        TypedRecordKind::TXT(TxtRecordData {
            value: vec![
                "v=spf1".into(),
                "include:_spf.example.org".into(),
                "-all".into(),
            ],
        })
    )]
    #[case::txt_with_empty_values(
        "TXT",
        json!({"value": []}),
        TypedRecordKind::TXT(TxtRecordData { value: vec![] })
    )]
    fn edge_case_payload_round_trips_through_typed(
        #[case] type_name: &str,
        #[case] data: Value,
        #[case] expected: TypedRecordKind,
    ) {
        assert_typed_round_trip(type_name, data, expected);
    }

    // ---- Opaque (RFC 3597 / user-registered) fallback via outer enum. ----

    #[rstest]
    #[case::unknown_type_with_object_data(
        json!({"type_name": "TYPE65400", "data": {"raw": "abc"}}),
        RecordKind::Opaque(OpaqueRecordKind {
            type_name: "TYPE65400".into(),
            data: Some(json!({"raw": "abc"})),
        })
    )]
    #[case::known_type_with_invalid_data(
        json!({"type_name": "A", "data": {"address": 42}}),
        RecordKind::Opaque(OpaqueRecordKind {
            type_name: "A".into(),
            data: Some(json!({"address": 42})),
        })
    )]
    fn payload_deserializes_as_opaque_record_kind(
        #[case] payload: Value,
        #[case] expected: RecordKind,
    ) {
        let kind: RecordKind = serde_json::from_value(payload).expect("deserialize as RecordKind");
        assert_eq!(kind, expected);
    }

    #[test]
    fn unknown_type_with_null_data_round_trips() {
        let kind = RecordKind::Opaque(OpaqueRecordKind {
            type_name: "TYPE65400".into(),
            data: None,
        });
        let serialized = serde_json::to_value(&kind).expect("serialize");
        let expected = json!({"type_name": "TYPE65400", "data": null});
        assert_eq!(serialized, expected);
        let deserialized: RecordKind = serde_json::from_value(expected).expect("deserialize");
        assert_eq!(deserialized, kind);
    }

    // ---- Drift: every built-in must have a typed variant + a sample test case ----

    fn typed_sample_type_names() -> HashSet<&'static str> {
        TYPED_SCHEMA_MANIFEST
            .iter()
            .map(|entry| entry.type_name)
            .collect()
    }

    #[test]
    fn every_builtin_type_has_typed_sample() {
        let builtins = built_in_record_types().expect("built-ins must construct");
        let known = typed_sample_type_names();

        let builtin_names: HashSet<String> = builtins
            .iter()
            .map(|definition| definition.name().as_str().to_string())
            .collect();

        for name in &builtin_names {
            assert!(
                known.contains(name.as_str()),
                "drift: built-in record type '{name}' is not represented in TypedRecordKind \
                 + tests::typed_sample_type_names. Add a per-type rstest case and a \
                 corresponding TypedRecordKind variant.",
            );
        }

        for sample_name in &known {
            assert!(
                builtin_names.contains(*sample_name),
                "stale sample: tests::typed_sample_type_names lists '{sample_name}' but it is \
                 no longer a built-in record type.",
            );
        }
    }

    #[test]
    fn typed_record_kind_variant_count_matches_builtins() {
        let builtin_count = built_in_record_types().expect("built-ins").len();
        let sample_count = typed_sample_type_names().len();
        assert_eq!(
            sample_count, builtin_count,
            "TypedRecordKind/sample count diverged from built_in_record_types()",
        );
    }

    #[test]
    fn openapi_document_publishes_typed_record_schemas() {
        // Confirms every per-type schema is wired into the OpenAPI document
        // published at /api-docs/openapi.json.  The shape is `oneOf` without
        // a discriminator object — that is by design (see module docs); this
        // test only verifies presence, not discriminator semantics.
        use utoipa::OpenApi;
        let doc = serde_json::to_string(&crate::api::ApiDoc::openapi())
            .expect("openapi doc must serialize");

        // Outer + opaque envelope.
        assert!(doc.contains("\"RecordKind\""), "RecordKind schema missing");
        assert!(
            doc.contains("\"TypedRecordKind\""),
            "TypedRecordKind schema missing",
        );
        assert!(
            doc.contains("\"OpaqueRecordKind\""),
            "OpaqueRecordKind schema missing",
        );

        // Spot-check every per-type data schema is present.  Using the same
        // sample list as the drift test guarantees these two stay in sync.
        for entry in TYPED_SCHEMA_MANIFEST {
            assert!(
                doc.contains(&format!("\"{}\"", entry.schema_name)),
                "OpenAPI doc missing per-type schema {} for {}",
                entry.schema_name,
                entry.type_name,
            );
        }
    }

    #[test]
    fn openapi_record_response_flattens_record_kind_without_discriminator_or_stale_fields() {
        use utoipa::OpenApi;
        let doc = serde_json::to_value(crate::api::ApiDoc::openapi())
            .expect("openapi doc must serialize as JSON value");

        let record_response = doc
            .pointer("/components/schemas/RecordResponse")
            .expect("RecordResponse schema must be published");
        let all_of = record_response
            .get("allOf")
            .and_then(Value::as_array)
            .expect("RecordResponse must be expressed as allOf");
        assert_eq!(
            all_of.len(),
            2,
            "RecordResponse should have two allOf parts"
        );
        assert_eq!(
            all_of[0].get("$ref").and_then(Value::as_str),
            Some("#/components/schemas/RecordKind"),
            "RecordResponse must flatten RecordKind as its first allOf part",
        );

        let response_props = all_of[1]
            .get("properties")
            .and_then(Value::as_object)
            .expect("RecordResponse object half must publish properties");
        assert!(
            !response_props.contains_key("kind"),
            "flattened kind must not leak a stale `kind` property",
        );
        assert!(
            !response_props.contains_key("type_name"),
            "type_name should come from RecordKind, not a stale RecordResponse field",
        );
        assert!(
            !response_props.contains_key("data"),
            "data should come from RecordKind, not a stale RecordResponse field",
        );

        let record_kind = doc
            .pointer("/components/schemas/RecordKind")
            .expect("RecordKind schema must be published");
        assert!(
            record_kind.get("discriminator").is_none(),
            "RecordKind must not advertise an OpenAPI discriminator",
        );
        let typed_kind = doc
            .pointer("/components/schemas/TypedRecordKind")
            .expect("TypedRecordKind schema must be published");
        assert!(
            typed_kind.get("discriminator").is_none(),
            "TypedRecordKind must not advertise an OpenAPI discriminator in the current model",
        );

        let typed_variant_props = typed_kind
            .pointer("/oneOf/0/properties")
            .and_then(Value::as_object)
            .expect("TypedRecordKind variants must publish properties");
        assert!(
            typed_variant_props.contains_key("type_name")
                && typed_variant_props.contains_key("data"),
            "typed union variants must carry the flattened type_name/data fields",
        );

        let opaque_props = doc
            .pointer("/components/schemas/OpaqueRecordKind/properties")
            .and_then(Value::as_object)
            .expect("OpaqueRecordKind must publish properties");
        assert!(
            opaque_props.contains_key("type_name") && opaque_props.contains_key("data"),
            "opaque fallback must carry the flattened type_name/data fields",
        );
    }

    #[test]
    fn openapi_opaque_record_data_is_published_as_nullable() {
        // Regression for a published-schema vs wire-shape mismatch:
        // `RecordKind::from_record` emits `data: null` for raw-rdata / RFC 3597
        // responses, so the schema for OpaqueRecordKind.data MUST be nullable.
        use utoipa::OpenApi;
        let doc = serde_json::to_value(crate::api::ApiDoc::openapi())
            .expect("openapi doc must serialize as JSON value");

        let schema = doc
            .pointer("/components/schemas/OpaqueRecordKind")
            .expect("OpaqueRecordKind schema must be published");
        let data_field = schema
            .pointer("/properties/data")
            .expect("OpaqueRecordKind.data property must be published");

        // Accept either OpenAPI 3.0 (`nullable: true`) or 3.1 (`type: ["object", "null"]`)
        // so this test stays useful across utoipa version bumps.
        let nullable_30 = data_field
            .get("nullable")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let nullable_31 = data_field
            .get("type")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().any(|v| v.as_str() == Some("null")))
            .unwrap_or(false);

        assert!(
            nullable_30 || nullable_31,
            "OpaqueRecordKind.data must be declared nullable; got {data_field}",
        );

        // And it must NOT be in `required`, since `RecordKind::from_record` also
        // produces `data: null` (present but null) — that is satisfied either
        // way, but we explicitly forbid required to keep the schema honest.
        let required_present = schema
            .get("required")
            .and_then(Value::as_array)
            .map(|arr| arr.iter().any(|v| v.as_str() == Some("data")))
            .unwrap_or(false);
        assert!(
            !required_present,
            "OpaqueRecordKind.data must not be marked required",
        );
    }
}
