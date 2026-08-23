use std::collections::BTreeSet;

use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use serde_json::{Value, json};
use url::Url;

use crate::{
    domain::{
        resource_records::{
            ExistingRecordSummary, RecordCardinality, RecordFieldKind, RecordFieldSchema,
            RecordOwnerNameSyntax, RecordRfcProfile, RecordTypeDefinition, ValidatedRecordContent,
        },
        types::{
            DnsCharacterString, DnsName, DomainNameValue, HexEncodedValue, Hostname, Ipv4AddrValue,
            Ipv6AddrValue, RecordTypeName,
        },
    },
    errors::AppError,
};

/// Validates cross-record constraints (CNAME exclusivity, RRSet TTL matching,
/// duplicate detection, null MX rules, and alias target restrictions).
pub fn validate_record_relationships(
    record_type: &RecordTypeDefinition,
    ttl: Option<crate::domain::types::Ttl>,
    content: &ValidatedRecordContent,
    same_owner_records: &[ExistingRecordSummary],
    same_rrset_records: &[ExistingRecordSummary],
    alias_owner_names: &BTreeSet<String>,
) -> Result<(), AppError> {
    check_cardinality(record_type, same_rrset_records)?;
    check_rrset_ttl_match(record_type, ttl, same_rrset_records)?;
    check_cname_exclusivity(record_type, same_owner_records)?;
    check_duplicate_rr(same_rrset_records, content)?;
    check_null_mx(record_type, content, same_rrset_records)?;
    check_child_ds_delete_signal(record_type, content, same_rrset_records)?;
    check_alias_targets(record_type, content, alias_owner_names)?;
    Ok(())
}

fn check_cardinality(
    record_type: &RecordTypeDefinition,
    same_rrset_records: &[ExistingRecordSummary],
) -> Result<(), AppError> {
    if matches!(
        record_type.schema().cardinality(),
        RecordCardinality::Single
    ) && !same_rrset_records.is_empty()
    {
        return Err(AppError::conflict(
            "record type is single-valued for this owner",
        ));
    }
    Ok(())
}

fn check_rrset_ttl_match(
    record_type: &RecordTypeDefinition,
    ttl: Option<crate::domain::types::Ttl>,
    same_rrset_records: &[ExistingRecordSummary],
) -> Result<(), AppError> {
    if record_type
        .schema()
        .rfc_profile()?
        .as_ref()
        .is_some_and(RecordRfcProfile::rrset_ttl_must_match)
        && let Some(existing) = same_rrset_records.first()
        && ttl != existing.ttl()
    {
        return Err(AppError::validation(
            "all records in an RRSet must use the same TTL",
        ));
    }
    Ok(())
}

fn check_cname_exclusivity(
    record_type: &RecordTypeDefinition,
    same_owner_records: &[ExistingRecordSummary],
) -> Result<(), AppError> {
    let type_name = record_type.name().as_str();
    let is_cname = type_name == "CNAME";
    let owner_has_cname = same_owner_records
        .iter()
        .any(|record| record.type_name().as_str() == "CNAME");
    if is_cname && !same_owner_records.is_empty() {
        return Err(AppError::conflict(format!(
            "a {type_name} record cannot coexist with other data at the same owner name",
        )));
    }
    if !is_cname && owner_has_cname {
        return Err(AppError::conflict(
            "an owner name with a CNAME record cannot hold other record types",
        ));
    }
    Ok(())
}

/// Validate relationships that span owner names in the DNS graph.
pub fn validate_alias_graph(
    type_name: &RecordTypeName,
    owner_name: &DnsName,
    has_inbound_alias_restricted_reference: bool,
    has_descendant_data: bool,
    is_below_dname: bool,
) -> Result<(), AppError> {
    if type_name.as_str() == "CNAME" && has_inbound_alias_restricted_reference {
        return Err(AppError::conflict(format!(
            "cannot create CNAME '{}': existing NS, MX, PTR, SRV, or NAPTR data references it",
            owner_name.as_str()
        )));
    }
    if type_name.as_str() == "DNAME" && has_descendant_data {
        return Err(AppError::conflict(format!(
            "cannot create DNAME '{}': descendant owner names already contain data",
            owner_name.as_str()
        )));
    }
    if is_below_dname {
        return Err(AppError::conflict(format!(
            "owner '{}' is below an existing DNAME and cannot contain authoritative data",
            owner_name.as_str()
        )));
    }
    Ok(())
}

fn check_duplicate_rr(
    same_rrset_records: &[ExistingRecordSummary],
    content: &ValidatedRecordContent,
) -> Result<(), AppError> {
    if same_rrset_records
        .iter()
        .any(|existing| record_payloads_match(existing, content))
    {
        return Err(AppError::conflict(
            "identical duplicate resource records are not allowed in the same RRSet",
        ));
    }
    Ok(())
}

fn check_null_mx(
    record_type: &RecordTypeDefinition,
    content: &ValidatedRecordContent,
    same_rrset_records: &[ExistingRecordSummary],
) -> Result<(), AppError> {
    if record_type.name().as_str() != "MX" {
        return Ok(());
    }
    let normalized = match content {
        ValidatedRecordContent::Structured(value) => value,
        ValidatedRecordContent::RawRdata(_) => {
            return Ok(());
        }
    };
    let is_null_mx = normalized
        .get("exchange")
        .and_then(Value::as_str)
        .is_some_and(|exchange| exchange == ".")
        && normalized
            .get("preference")
            .and_then(Value::as_u64)
            .is_some_and(|preference| preference == 0);

    if is_null_mx && !same_rrset_records.is_empty() {
        return Err(AppError::conflict(
            "a null MX RRSet cannot coexist with other MX records",
        ));
    }

    if !is_null_mx
        && same_rrset_records.iter().any(|record| {
            record.data().get("exchange").and_then(Value::as_str) == Some(".")
                && record.data().get("preference").and_then(Value::as_u64) == Some(0)
        })
    {
        return Err(AppError::conflict(
            "an MX RRSet containing a null MX record cannot accept other MX records",
        ));
    }
    Ok(())
}

fn check_child_ds_delete_signal(
    record_type: &RecordTypeDefinition,
    content: &ValidatedRecordContent,
    same_rrset_records: &[ExistingRecordSummary],
) -> Result<(), AppError> {
    let type_name = record_type.name().as_str();
    if !matches!(type_name, "CDS" | "CDNSKEY") {
        return Ok(());
    }
    let incoming_is_delete = match content {
        ValidatedRecordContent::Structured(data) => is_child_ds_delete_signal(type_name, data),
        ValidatedRecordContent::RawRdata(_) => false,
    };
    let existing_has_delete = same_rrset_records
        .iter()
        .any(|record| is_child_ds_delete_signal(type_name, record.data()));
    if (incoming_is_delete && !same_rrset_records.is_empty())
        || (!incoming_is_delete && existing_has_delete)
    {
        return Err(AppError::conflict(format!(
            "an RFC 8078 {type_name} delete signal must be the only record in its RRSet"
        )));
    }
    Ok(())
}

fn is_child_ds_delete_signal(type_name: &str, data: &Value) -> bool {
    match type_name {
        "CDS" => {
            data.get("key_tag").and_then(Value::as_u64) == Some(0)
                && data.get("algorithm").and_then(Value::as_u64) == Some(0)
                && data.get("digest_type").and_then(Value::as_u64) == Some(0)
                && data.get("digest").and_then(Value::as_str) == Some("00")
        }
        "CDNSKEY" => {
            data.get("flags").and_then(Value::as_u64) == Some(0)
                && data.get("protocol").and_then(Value::as_u64) == Some(3)
                && data.get("algorithm").and_then(Value::as_u64) == Some(0)
                && data
                    .get("public_key")
                    .and_then(Value::as_str)
                    .and_then(|value| BASE64.decode(value).ok())
                    .is_some_and(|decoded| decoded == [0])
        }
        _ => false,
    }
}

fn check_alias_targets(
    record_type: &RecordTypeDefinition,
    content: &ValidatedRecordContent,
    alias_owner_names: &BTreeSet<String>,
) -> Result<(), AppError> {
    if let Some(profile) = record_type.schema().rfc_profile()? {
        let normalized = match content {
            ValidatedRecordContent::Structured(value) => value,
            ValidatedRecordContent::RawRdata(_) => return Ok(()),
        };
        for field in profile.target_fields_must_not_be_aliases() {
            if normalized
                .get(field)
                .and_then(Value::as_str)
                .is_some_and(|target| target != "." && alias_owner_names.contains(target))
            {
                return Err(AppError::validation(format!(
                    "record field '{}' must not reference an alias target",
                    field
                )));
            }
        }
    }
    Ok(())
}

pub(crate) fn validate_field_value(
    field: &RecordFieldSchema,
    value: &Value,
) -> Result<Value, AppError> {
    match field.kind() {
        RecordFieldKind::Fqdn => validate_fqdn_field(field, value),
        RecordFieldKind::DomainName => validate_domain_name_field(field, value),
        RecordFieldKind::Ipv4 => validate_ipv4_field(field, value),
        RecordFieldKind::Ipv6 => validate_ipv6_field(field, value),
        RecordFieldKind::Uint16 => validate_uint16_field(field, value),
        RecordFieldKind::Uint32 => validate_uint32_field(field, value),
        RecordFieldKind::Float64 => validate_float64_field(field, value),
        RecordFieldKind::Enum => validate_enum_field(field, value),
        RecordFieldKind::Text => validate_text_field(field, value),
        RecordFieldKind::CharString => validate_char_string_field(field, value),
        RecordFieldKind::Hex => validate_hex_field(field, value),
        RecordFieldKind::List => validate_list_field(field, value),
        RecordFieldKind::Boolean => validate_boolean_field(field, value),
    }
}

fn validate_fqdn_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(DnsName::new(raw)?.to_string()))
}

fn validate_domain_name_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(DomainNameValue::new(raw)?.to_string()))
}

fn validate_ipv4_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(Ipv4AddrValue::new(raw)?.to_string()))
}

fn validate_ipv6_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(Ipv6AddrValue::new(raw)?.to_string()))
}

fn validate_uint16_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let number = value.as_u64().ok_or_else(|| {
        AppError::validation(format!(
            "record field '{}' must be an integer",
            field.name()
        ))
    })?;
    if number > u16::MAX as u64 {
        return Err(AppError::validation(format!(
            "record field '{}' exceeds uint16 range",
            field.name()
        )));
    }
    Ok(Value::Number(number.into()))
}

fn validate_uint32_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let number = value.as_u64().ok_or_else(|| {
        AppError::validation(format!(
            "record field '{}' must be an integer",
            field.name()
        ))
    })?;
    if number > u32::MAX as u64 {
        return Err(AppError::validation(format!(
            "record field '{}' exceeds uint32 range",
            field.name()
        )));
    }
    Ok(Value::Number(number.into()))
}

fn validate_float64_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let number = value.as_f64().ok_or_else(|| {
        AppError::validation(format!(
            "record field '{}' must be a floating point number",
            field.name()
        ))
    })?;
    if !number.is_finite() {
        return Err(AppError::validation(format!(
            "record field '{}' must be finite",
            field.name()
        )));
    }
    Ok(json!(number))
}

fn validate_enum_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    if !field.options().iter().any(|option| option == raw) {
        return Err(AppError::validation(format!(
            "record field '{}' must be one of {:?}",
            field.name(),
            field.options()
        )));
    }
    Ok(Value::String(raw.to_string()))
}

fn validate_text_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(raw.to_string()))
}

fn validate_char_string_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(
        DnsCharacterString::new(raw.to_string())?.to_string(),
    ))
}

fn validate_hex_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let raw = value.as_str().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a string", field.name()))
    })?;
    Ok(Value::String(HexEncodedValue::new(raw)?.to_string()))
}

fn validate_list_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    let items = value.as_array().ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be an array", field.name()))
    })?;
    Ok(Value::Array(items.clone()))
}

fn validate_boolean_field(field: &RecordFieldSchema, value: &Value) -> Result<Value, AppError> {
    value.as_bool().map(Value::Bool).ok_or_else(|| {
        AppError::validation(format!("record field '{}' must be a boolean", field.name()))
    })
}

pub(crate) fn preprocess_builtin_payload(
    type_name: &RecordTypeName,
    payload: &Value,
) -> Result<Value, AppError> {
    let object = payload
        .as_object()
        .ok_or_else(|| AppError::validation("record payload must be a JSON object"))?;
    let mut normalized = object.clone();
    match type_name.as_str() {
        "NAPTR" => {
            if let Some(service) = normalized.remove("service")
                && !normalized.contains_key("services")
            {
                normalized.insert("services".to_string(), service);
            }
        }
        "CAA" => {
            if let Some(tag) = normalized.get_mut("tag")
                && let Some(value) = tag.as_str()
            {
                *tag = Value::String(value.to_ascii_lowercase());
            }
        }
        _ => {}
    }
    Ok(Value::Object(normalized))
}

pub(crate) fn validate_owner_name(
    type_name: &RecordTypeName,
    owner_name: &str,
    profile: Option<&RecordRfcProfile>,
) -> Result<(), AppError> {
    let Some(profile) = profile else {
        DnsName::new(owner_name)?;
        return Ok(());
    };

    match profile.owner_name_syntax() {
        RecordOwnerNameSyntax::DnsName => {
            DnsName::new(owner_name)?;
        }
        RecordOwnerNameSyntax::Hostname => {
            Hostname::new(owner_name)?;
        }
    }

    if type_name.as_str() == "SRV" {
        let labels = owner_name.split('.').collect::<Vec<_>>();
        if labels.len() < 3
            || !labels[0].starts_with('_')
            || labels[0].len() == 1
            || !labels[1].starts_with('_')
            || labels[1].len() == 1
        {
            return Err(AppError::validation(
                "SRV owner names must start with non-empty _service._proto labels",
            ));
        }
    }
    if type_name.as_str() == "URI" {
        let first_label = owner_name.split('.').next().unwrap_or_default();
        if !first_label.starts_with('_') || first_label.len() == 1 {
            return Err(AppError::validation(
                "URI owner names must start with a non-empty underscored service label",
            ));
        }
    }
    if type_name.as_str() == "TLSA" {
        validate_tlsa_owner_name(owner_name)?;
    }
    if matches!(type_name.as_str(), "SMIMEA" | "OPENPGPKEY") {
        validate_email_security_owner_name(type_name.as_str(), owner_name)?;
    }

    Ok(())
}

fn validate_tlsa_owner_name(owner_name: &str) -> Result<(), AppError> {
    let labels = owner_name.split('.').collect::<Vec<_>>();
    let port = labels
        .first()
        .and_then(|label| label.strip_prefix('_'))
        .filter(|port| !port.is_empty() && (*port == "0" || !port.starts_with('0')))
        .and_then(|port| port.parse::<u16>().ok());
    let transport = labels.get(1).copied();
    if port.is_none() || !matches!(transport, Some("_tcp" | "_udp" | "_sctp")) || labels.len() < 3 {
        return Err(AppError::validation(
            "TLSA owner names must use _<port>._<tcp|udp|sctp>.<hostname>",
        ));
    }
    Hostname::new(labels[2..].join("."))?;
    Ok(())
}

fn validate_email_security_owner_name(type_name: &str, owner_name: &str) -> Result<(), AppError> {
    let labels = owner_name.split('.').collect::<Vec<_>>();
    let expected_leaf = if type_name == "SMIMEA" {
        "_smimecert"
    } else {
        "_openpgpkey"
    };
    if labels.len() < 3
        || labels[0].len() != 56
        || !labels[0]
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        || labels[1] != expected_leaf
    {
        return Err(AppError::validation(format!(
            "{type_name} owner names must use a 56-hex-character local-part hash followed by {expected_leaf}"
        )));
    }
    Hostname::new(labels[2..].join("."))?;
    Ok(())
}

pub(crate) fn validate_builtin_payload(
    type_name: &RecordTypeName,
    normalized: &Value,
) -> Result<Value, AppError> {
    match type_name.as_str() {
        "MX" => validate_mx_payload(normalized),
        "NAPTR" => validate_naptr_payload(normalized),
        "SSHFP" => validate_sshfp_payload(normalized),
        "LOC" => validate_loc_payload(normalized),
        "DS" => validate_ds_payload(normalized, false),
        "CDS" => validate_ds_payload(normalized, true),
        "DNSKEY" => validate_dnskey_payload(normalized, false),
        "CDNSKEY" => validate_dnskey_payload(normalized, true),
        "SMIMEA" => validate_tlsa_payload(normalized),
        "CAA" => validate_caa_payload(normalized),
        "TLSA" => validate_tlsa_payload(normalized),
        "SVCB" | "HTTPS" => validate_svcb_payload(normalized),
        "CSYNC" => validate_csync_payload(normalized),
        "URI" => validate_uri_payload(normalized),
        "OPENPGPKEY" => validate_openpgpkey_payload(normalized),
        _ => Ok(normalized.clone()),
    }
}

pub(crate) fn validate_mx_payload(normalized: &Value) -> Result<Value, AppError> {
    let preference = normalized
        .get("preference")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("MX preference is required"))?;
    let exchange = DomainNameValue::new(
        normalized
            .get("exchange")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("MX exchange is required"))?,
    )?;
    if exchange.is_root() && preference != 0 {
        return Err(AppError::validation(
            "a null MX record must use preference 0 and exchange '.'",
        ));
    }
    Ok(json!({
        "preference": preference,
        "exchange": exchange.as_str(),
    }))
}

pub(crate) fn validate_naptr_payload(normalized: &Value) -> Result<Value, AppError> {
    let order = normalized
        .get("order")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("NAPTR order is required"))?;
    let preference = normalized
        .get("preference")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("NAPTR preference is required"))?;
    let flags = DnsCharacterString::new(
        normalized
            .get("flags")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("NAPTR flags are required"))?
            .to_string(),
    )?;
    let services = DnsCharacterString::new(
        normalized
            .get("services")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("NAPTR services are required"))?
            .to_string(),
    )?;
    let regexp = DnsCharacterString::new(
        normalized
            .get("regexp")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("NAPTR regexp is required"))?
            .to_string(),
    )?;
    let replacement = DomainNameValue::new(
        normalized
            .get("replacement")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("NAPTR replacement is required"))?,
    )?;

    let has_regexp = !regexp.as_str().is_empty();
    let has_replacement = !replacement.is_root();
    if has_regexp == has_replacement {
        return Err(AppError::validation(
            "NAPTR records must use exactly one of a non-empty regexp or a non-root replacement",
        ));
    }

    Ok(json!({
        "order": order,
        "preference": preference,
        "flags": flags.as_str(),
        "services": services.as_str(),
        "regexp": regexp.as_str(),
        "replacement": replacement.as_str(),
    }))
}

pub(crate) fn validate_sshfp_payload(normalized: &Value) -> Result<Value, AppError> {
    let algorithm = normalized
        .get("algorithm")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("SSHFP algorithm is required"))?;
    let fp_type = normalized
        .get("fp_type")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("SSHFP fp_type is required"))?;
    if !matches!(algorithm, 1 | 2 | 3 | 4 | 6) {
        return Err(AppError::validation(
            "SSHFP algorithm must be one of the currently supported IANA values: 1, 2, 3, 4, 6",
        ));
    }
    if !matches!(fp_type, 1 | 2) {
        return Err(AppError::validation(
            "SSHFP fp_type must be one of the currently supported IANA values: 1 or 2",
        ));
    }
    let fingerprint = HexEncodedValue::new(
        normalized
            .get("fingerprint")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("SSHFP fingerprint is required"))?,
    )?;

    // Validate fingerprint length matches the digest type (RFC 4255/6594)
    let expected_hex_len = match fp_type {
        1 => 40, // SHA-1: 20 bytes = 40 hex chars
        2 => 64, // SHA-256: 32 bytes = 64 hex chars
        _ => 0,  // unreachable due to check above
    };
    if expected_hex_len > 0 && fingerprint.as_str().len() != expected_hex_len {
        return Err(AppError::validation(format!(
            "SSHFP fingerprint must be {} hex characters for fp_type {} (got {})",
            expected_hex_len,
            fp_type,
            fingerprint.as_str().len()
        )));
    }

    Ok(json!({
        "algorithm": algorithm,
        "fp_type": fp_type,
        "fingerprint": fingerprint.as_str(),
    }))
}

pub(crate) fn validate_loc_payload(normalized: &Value) -> Result<Value, AppError> {
    let latitude = get_f64_field(normalized, "latitude")?;
    let longitude = get_f64_field(normalized, "longitude")?;
    let altitude_m = get_f64_field(normalized, "altitude_m")?;
    let size_m = get_optional_f64_field(normalized, "size_m")?.unwrap_or(1.0);
    let horizontal_precision_m =
        get_optional_f64_field(normalized, "horizontal_precision_m")?.unwrap_or(10_000.0);
    let vertical_precision_m =
        get_optional_f64_field(normalized, "vertical_precision_m")?.unwrap_or(10.0);

    if !(-90.0..=90.0).contains(&latitude) {
        return Err(AppError::validation(
            "LOC latitude must be between -90 and 90",
        ));
    }
    if !(-180.0..=180.0).contains(&longitude) {
        return Err(AppError::validation(
            "LOC longitude must be between -180 and 180",
        ));
    }
    if !(-100_000.0..=42_849_672.95).contains(&altitude_m) {
        return Err(AppError::validation(
            "LOC altitude_m must be between -100000 and 42849672.95",
        ));
    }
    for (field, value) in [
        ("size_m", size_m),
        ("horizontal_precision_m", horizontal_precision_m),
        ("vertical_precision_m", vertical_precision_m),
    ] {
        if !(0.01..=90_000_000.0).contains(&value) {
            return Err(AppError::validation(format!(
                "LOC {} must be between 0.01 and 90000000 metres",
                field
            )));
        }
    }

    Ok(json!({
        "latitude": latitude,
        "longitude": longitude,
        "altitude_m": altitude_m,
        "size_m": size_m,
        "horizontal_precision_m": horizontal_precision_m,
        "vertical_precision_m": vertical_precision_m,
    }))
}

pub(crate) fn validate_ds_payload(
    normalized: &Value,
    allow_delete_signal: bool,
) -> Result<Value, AppError> {
    let key_tag = normalized
        .get("key_tag")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DS key_tag is required"))?;
    let algorithm = normalized
        .get("algorithm")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DS algorithm is required"))?;
    let digest_type = normalized
        .get("digest_type")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DS digest_type is required"))?;
    let digest_raw = normalized
        .get("digest")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("DS digest is required"))?;

    if allow_delete_signal && key_tag == 0 && algorithm == 0 && digest_type == 0 {
        if digest_raw != "00" {
            return Err(AppError::validation(
                "an RFC 8078 CDS delete signal must be exactly '0 0 0 00'",
            ));
        }
        return Ok(json!({
            "key_tag": 0,
            "algorithm": 0,
            "digest_type": 0,
            "digest": "00",
        }));
    }

    if !is_dnssec_zone_signing_algorithm(algorithm) {
        return Err(AppError::validation(
            "DS algorithm is not usable for zone signing in the DNS Security Algorithm Numbers registry",
        ));
    }

    let expected_digest_len = match digest_type {
        1 => Some(40),
        2 | 3 | 5 | 6 => Some(64),
        4 => Some(96),
        253 | 254 => None,
        _ => {
            return Err(AppError::validation(
                "DS digest_type is neither assigned nor private-use in the DS RR Type Digest Algorithms registry",
            ));
        }
    };
    let digest = HexEncodedValue::new(digest_raw)?;
    if digest.as_str().is_empty() {
        return Err(AppError::validation("DS digest cannot be empty"));
    }
    if expected_digest_len.is_some_and(|length| digest.as_str().len() != length) {
        return Err(AppError::validation(format!(
            "DS digest has the wrong hexadecimal length for digest_type {digest_type}"
        )));
    }

    Ok(json!({
        "key_tag": key_tag,
        "algorithm": algorithm,
        "digest_type": digest_type,
        "digest": digest.as_str(),
    }))
}

pub(crate) fn validate_dnskey_payload(
    normalized: &Value,
    allow_delete_signal: bool,
) -> Result<Value, AppError> {
    let flags = normalized
        .get("flags")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DNSKEY flags is required"))?;
    let protocol = normalized
        .get("protocol")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DNSKEY protocol is required"))?;
    let algorithm = normalized
        .get("algorithm")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("DNSKEY algorithm is required"))?;

    // RFC 4034 Section 2.1.2: protocol MUST be 3
    if protocol != 3 {
        return Err(AppError::validation(
            "DNSKEY protocol must be 3 (RFC 4034 Section 2.1.2)",
        ));
    }

    // Flags: bit 7 (Zone Key) and bit 15 (SEP) are the meaningful bits
    // Valid values: 256 (ZSK), 257 (KSK/CSK), 0 (non-zone key)
    if flags > u16::MAX as u64 {
        return Err(AppError::validation("DNSKEY flags exceeds uint16 range"));
    }

    let public_key = normalized
        .get("public_key")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("DNSKEY public_key is required"))?;

    let public_key_bytes = BASE64.decode(public_key).map_err(|error| {
        AppError::validation(format!("DNSKEY public_key is not base64: {error}"))
    })?;
    if public_key_bytes.is_empty() {
        return Err(AppError::validation(
            "DNSKEY public_key cannot decode to empty data",
        ));
    }
    if allow_delete_signal && flags == 0 && algorithm == 0 {
        if public_key_bytes != [0] {
            return Err(AppError::validation(
                "an RFC 8078 CDNSKEY delete signal must be exactly '0 3 0 AA=='",
            ));
        }
        return Ok(json!({
            "flags": 0,
            "protocol": 3,
            "algorithm": 0,
            "public_key": "AA==",
        }));
    }
    if !is_dnssec_zone_signing_algorithm(algorithm) {
        return Err(AppError::validation(
            "DNSKEY algorithm is not usable for zone signing in the DNS Security Algorithm Numbers registry",
        ));
    }
    let expected_len = match algorithm {
        13 => Some(64),
        14 => Some(96),
        15 => Some(32),
        16 => Some(57),
        _ => None,
    };
    if expected_len.is_some_and(|length| public_key_bytes.len() != length) {
        return Err(AppError::validation(format!(
            "DNSKEY public_key has the wrong decoded length for algorithm {algorithm}"
        )));
    }

    Ok(json!({
        "flags": flags,
        "protocol": protocol,
        "algorithm": algorithm,
        "public_key": public_key,
    }))
}

pub(crate) fn validate_caa_payload(normalized: &Value) -> Result<Value, AppError> {
    let flags = normalized
        .get("flags")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("CAA flags is required"))?;
    if !matches!(flags, 0 | 128) {
        return Err(AppError::validation(
            "CAA flags may only contain the issuer-critical bit (0 or 128)",
        ));
    }
    let tag = normalized
        .get("tag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("CAA tag is required"))?;
    // RFC 8659: tags are case-insensitive and are normalized to lowercase.
    if !tag
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        || tag.is_empty()
    {
        return Err(AppError::validation(
            "CAA tag must be non-empty lowercase ASCII alphanumeric (e.g., 'issue', 'issuewild', 'iodef')",
        ));
    }
    let value = normalized
        .get("value")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("CAA value is required"))?;

    Ok(json!({
        "flags": flags,
        "tag": tag,
        "value": value,
    }))
}

pub(crate) fn validate_tlsa_payload(normalized: &Value) -> Result<Value, AppError> {
    let usage = normalized
        .get("usage")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("TLSA usage is required"))?;
    let selector = normalized
        .get("selector")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("TLSA selector is required"))?;
    let matching_type = normalized
        .get("matching_type")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("TLSA matching_type is required"))?;

    // RFC 6698 Section 2.1
    if !matches!(usage, 0..=3 | 255) {
        return Err(AppError::validation(
            "TLSA usage must be assigned (0-3) or private-use (255)",
        ));
    }
    if !matches!(selector, 0..=2 | 255) {
        return Err(AppError::validation(
            "TLSA selector must be assigned (0-2) or private-use (255)",
        ));
    }
    if !matches!(matching_type, 0..=2 | 255) {
        return Err(AppError::validation(
            "TLSA matching_type must be assigned (0-2) or private-use (255)",
        ));
    }

    let certificate_data = HexEncodedValue::new(
        normalized
            .get("certificate_data")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("TLSA certificate_data is required"))?,
    )?;
    let expected_hex_len = match matching_type {
        1 => Some(64),
        2 => Some(128),
        _ => None,
    };
    if expected_hex_len.is_some_and(|length| certificate_data.as_str().len() != length) {
        return Err(AppError::validation(format!(
            "TLSA certificate_data has the wrong digest length for matching_type {matching_type}"
        )));
    }

    Ok(json!({
        "usage": usage,
        "selector": selector,
        "matching_type": matching_type,
        "certificate_data": certificate_data.as_str(),
    }))
}

fn validate_csync_payload(normalized: &Value) -> Result<Value, AppError> {
    let soa_serial = normalized
        .get("soa_serial")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("CSYNC soa_serial is required"))?;
    let flags = normalized
        .get("flags")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("CSYNC flags is required"))?;
    if flags & !0x0003 != 0 {
        return Err(AppError::validation(
            "CSYNC flags may only contain the immediate and soaminimum bits",
        ));
    }
    let bitmap = normalized
        .get("type_bitmap")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("CSYNC type_bitmap is required"))?;
    let mut types = Vec::new();
    let mut unique = BTreeSet::new();
    for raw_type in bitmap.split_ascii_whitespace() {
        let rr_type = raw_type.to_ascii_uppercase();
        let mnemonic_is_valid = rr_type.chars().enumerate().all(|(index, character)| {
            character.is_ascii_uppercase()
                || character.is_ascii_digit() && index > 0
                || character == '-' && index > 0
        });
        let type_is_valid = match rr_type.strip_prefix("TYPE") {
            Some(number) if number.chars().all(|character| character.is_ascii_digit()) => number
                .parse::<u16>()
                .ok()
                .is_some_and(|number| number != 0 && number != u16::MAX),
            _ => mnemonic_is_valid,
        };
        if !type_is_valid || !unique.insert(rr_type.clone()) {
            return Err(AppError::validation(
                "CSYNC type_bitmap must contain unique RR type mnemonics or TYPE<number> values",
            ));
        }
        types.push(rr_type);
    }
    if types.is_empty() {
        return Err(AppError::validation(
            "CSYNC type_bitmap must contain at least one RR type",
        ));
    }
    Ok(json!({
        "soa_serial": soa_serial,
        "flags": flags,
        "type_bitmap": types.join(" "),
    }))
}

fn validate_uri_payload(normalized: &Value) -> Result<Value, AppError> {
    let priority = normalized
        .get("priority")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("URI priority is required"))?;
    let weight = normalized
        .get("weight")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("URI weight is required"))?;
    let target = normalized
        .get("target")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("URI target is required"))?;
    if target.is_empty() {
        return Err(AppError::validation("URI target cannot be empty"));
    }
    Url::parse(target)
        .map_err(|error| AppError::validation(format!("URI target is invalid: {error}")))?;
    Ok(json!({"priority": priority, "weight": weight, "target": target}))
}

fn validate_openpgpkey_payload(normalized: &Value) -> Result<Value, AppError> {
    let public_key = normalized
        .get("public_key")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("OPENPGPKEY public_key is required"))?;
    let decoded = BASE64.decode(public_key).map_err(|error| {
        AppError::validation(format!("OPENPGPKEY public_key is not base64: {error}"))
    })?;
    if decoded.is_empty() {
        return Err(AppError::validation(
            "OPENPGPKEY public_key cannot decode to empty data",
        ));
    }
    Ok(json!({"public_key": public_key}))
}

/// Algorithms whose current IANA registry entry permits DNSSEC zone signing.
///
/// This intentionally excludes assigned algorithms 1 and 2 because their
/// registry entries are not usable for zone signing. Private-use algorithms
/// remain legal DNSKEY/DS identifiers even though their key format is opaque.
fn is_dnssec_zone_signing_algorithm(algorithm: u64) -> bool {
    matches!(
        algorithm,
        3 | 5 | 6 | 7 | 8 | 10 | 12 | 13 | 14 | 15 | 16 | 17 | 18 | 23 | 253 | 254
    )
}

fn validate_svcb_payload(normalized: &Value) -> Result<Value, AppError> {
    let priority = normalized
        .get("priority")
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::validation("SVCB priority is required"))?;
    let target = DomainNameValue::new(
        normalized
            .get("target")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("SVCB target is required"))?,
    )?;
    let raw_params = normalized
        .get("params")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if priority == 0 && (target.is_root() || !raw_params.is_empty()) {
        return Err(AppError::validation(
            "SVCB AliasMode requires a non-root target and no service parameters",
        ));
    }

    let mut params = Vec::with_capacity(raw_params.len());
    let mut keys = BTreeSet::new();
    for raw in raw_params {
        let object = raw.as_object().ok_or_else(|| {
            AppError::validation("SVCB params entries must be objects with key and optional value")
        })?;
        if object.keys().any(|key| key != "key" && key != "value") {
            return Err(AppError::validation(
                "SVCB params entries only accept 'key' and 'value'",
            ));
        }
        let key = object
            .get("key")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("SVCB param key is required"))?
            .to_ascii_lowercase();
        let (key_number, key) = svcb_key(&key)?;
        if !keys.insert(key_number) {
            return Err(AppError::validation(format!(
                "duplicate SVCB parameter key '{key}'"
            )));
        }
        let value = object.get("value").and_then(Value::as_str);
        validate_svcb_param(key_number, &key, value)?;
        params.push((key_number, key, value.map(str::to_string)));
    }
    params.sort_by_key(|(number, _, _)| *number);

    if keys.contains(&2) && !keys.contains(&1) {
        return Err(AppError::validation(
            "SVCB no-default-alpn requires an alpn parameter",
        ));
    }
    if let Some((_, _, Some(mandatory))) = params.iter().find(|(number, _, _)| *number == 0) {
        let mut mandatory_keys = BTreeSet::new();
        for name in mandatory.split(',') {
            let (number, _) = svcb_key(name)?;
            if number == 0 || !mandatory_keys.insert(number) || !keys.contains(&number) {
                return Err(AppError::validation(
                    "SVCB mandatory must list unique parameters present in the same record and cannot list itself",
                ));
            }
        }
    }

    Ok(json!({
        "priority": priority,
        "target": target.as_str(),
        "params": params.into_iter().map(|(_, key, value)| {
            let mut entry = serde_json::Map::new();
            entry.insert("key".to_string(), Value::String(key));
            if let Some(value) = value {
                entry.insert("value".to_string(), Value::String(value));
            }
            Value::Object(entry)
        }).collect::<Vec<_>>(),
    }))
}

fn svcb_key(key: &str) -> Result<(u16, String), AppError> {
    let number = match key {
        "mandatory" => 0,
        "alpn" => 1,
        "no-default-alpn" => 2,
        "port" => 3,
        "ipv4hint" => 4,
        "ech" => 5,
        "ipv6hint" => 6,
        "dohpath" => 7,
        "ohttp" => 8,
        "tls-supported-groups" => 9,
        "docpath" => 10,
        "pvd" => 11,
        "oots" => 12,
        _ => key
            .strip_prefix("key")
            .filter(|number| !number.is_empty())
            .and_then(|number| number.parse::<u16>().ok())
            .ok_or_else(|| AppError::validation(format!("unknown SVCB parameter key '{key}'")))?,
    };
    if number == u16::MAX {
        return Err(AppError::validation(
            "SVCB parameter key 65535 is reserved as the invalid key",
        ));
    }
    Ok((number, svcb_key_name(number).unwrap_or(key).to_string()))
}

fn svcb_key_name(number: u16) -> Option<&'static str> {
    match number {
        0 => Some("mandatory"),
        1 => Some("alpn"),
        2 => Some("no-default-alpn"),
        3 => Some("port"),
        4 => Some("ipv4hint"),
        5 => Some("ech"),
        6 => Some("ipv6hint"),
        7 => Some("dohpath"),
        8 => Some("ohttp"),
        9 => Some("tls-supported-groups"),
        10 => Some("docpath"),
        11 => Some("pvd"),
        12 => Some("oots"),
        _ => None,
    }
}

fn validate_svcb_param(number: u16, key: &str, value: Option<&str>) -> Result<(), AppError> {
    match number {
        2 | 8 | 11 => {
            if value.is_some() {
                return Err(AppError::validation(format!(
                    "SVCB {key} must not have a value"
                )));
            }
        }
        3 => {
            value
                .ok_or_else(|| AppError::validation("SVCB port requires a value"))?
                .parse::<u16>()
                .map_err(|error| AppError::validation(format!("invalid SVCB port: {error}")))?;
        }
        4 => validate_address_hints(value, true)?,
        6 => validate_address_hints(value, false)?,
        1 => {
            let protocols = value
                .ok_or_else(|| AppError::validation("SVCB alpn requires a value"))?
                .split(',')
                .collect::<Vec<_>>();
            if protocols
                .iter()
                .any(|protocol| protocol.is_empty() || protocol.len() > 255)
            {
                return Err(AppError::validation(
                    "SVCB alpn protocol IDs must contain 1-255 octets",
                ));
            }
            if protocols.iter().collect::<BTreeSet<_>>().len() != protocols.len() {
                return Err(AppError::validation(
                    "SVCB alpn protocol IDs must be unique",
                ));
            }
        }
        0 | 5 | 7 | 9 | 12 if value.is_none() => {
            return Err(AppError::validation(format!(
                "SVCB parameter '{key}' requires a value"
            )));
        }
        _ => {}
    }
    Ok(())
}

fn validate_address_hints(value: Option<&str>, ipv4: bool) -> Result<(), AppError> {
    let value = value.ok_or_else(|| AppError::validation("SVCB address hints require a value"))?;
    if value.is_empty() {
        return Err(AppError::validation("SVCB address hints cannot be empty"));
    }
    for address in value.split(',') {
        let parsed = address.parse::<std::net::IpAddr>().map_err(|error| {
            AppError::validation(format!("invalid SVCB address hint '{address}': {error}"))
        })?;
        if parsed.is_ipv4() != ipv4 {
            return Err(AppError::validation(
                "SVCB address hint uses the wrong address family",
            ));
        }
    }
    Ok(())
}

pub(crate) fn get_f64_field(payload: &Value, field: &str) -> Result<f64, AppError> {
    payload
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| AppError::validation(format!("record field '{}' is required", field)))
}

pub(crate) fn get_optional_f64_field(
    payload: &Value,
    field: &str,
) -> Result<Option<f64>, AppError> {
    payload
        .get(field)
        .map(|value| {
            value.as_f64().ok_or_else(|| {
                AppError::validation(format!(
                    "record field '{}' must be a floating point number",
                    field
                ))
            })
        })
        .transpose()
}

/// Extract domain names referenced as alias targets from record data (MX exchange,
/// SRV target, NAPTR replacement, NS nsdname, PTR ptrdname).
pub fn alias_target_names(normalized: &Value, type_name: &RecordTypeName) -> Vec<String> {
    match type_name.as_str() {
        "MX" => normalized
            .get("exchange")
            .and_then(Value::as_str)
            .filter(|value| *value != ".")
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "SRV" => normalized
            .get("target")
            .and_then(Value::as_str)
            .filter(|value| *value != ".")
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "NAPTR" => normalized
            .get("replacement")
            .and_then(Value::as_str)
            .filter(|value| *value != ".")
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "NS" => normalized
            .get("nsdname")
            .and_then(Value::as_str)
            .filter(|value| *value != ".")
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        "PTR" => normalized
            .get("ptrdname")
            .and_then(Value::as_str)
            .filter(|value| *value != ".")
            .map(|value| vec![value.to_string()])
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

pub(crate) fn record_payloads_match(
    existing: &ExistingRecordSummary,
    incoming: &ValidatedRecordContent,
) -> bool {
    match incoming {
        ValidatedRecordContent::Structured(value) => {
            existing.raw_rdata().is_none() && existing.data() == value
        }
        ValidatedRecordContent::RawRdata(raw) => existing
            .raw_rdata()
            .is_some_and(|existing_raw| existing_raw == raw),
    }
}

pub(crate) fn allows_raw_rdata_from_flags(flags: &Value) -> bool {
    flags
        .get("rfc3597")
        .and_then(|value| value.get("allow_raw_rdata"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn decode_hex_bytes(value: &str) -> Result<Vec<u8>, AppError> {
    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let pair = std::str::from_utf8(pair).map_err(AppError::internal)?;
            u8::from_str_radix(pair, 16)
                .map_err(|error| AppError::validation(format!("invalid raw RDATA hex: {error}")))
        })
        .collect()
}

pub(crate) fn encode_hex_bytes(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
