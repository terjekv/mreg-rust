use std::collections::BTreeSet;

use serde_json::{Value, json};

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
        && let Some(existing_ttl) = same_rrset_records
            .iter()
            .find_map(ExistingRecordSummary::ttl)
        && ttl != Some(existing_ttl)
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
    let is_exclusive = type_name == "CNAME" || type_name == "DNAME";
    let owner_has_exclusive = same_owner_records.iter().any(|record| {
        let name = record.type_name().as_str();
        name == "CNAME" || name == "DNAME"
    });
    if is_exclusive && !same_owner_records.is_empty() {
        return Err(AppError::conflict(format!(
            "a {type_name} record cannot coexist with other data at the same owner name",
        )));
    }
    if !is_exclusive && owner_has_exclusive {
        return Err(AppError::conflict(
            "an owner name with a CNAME or DNAME record cannot hold other record types",
        ));
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
    if type_name.as_str() != "NAPTR" {
        return Ok(payload.clone());
    }

    let object = payload
        .as_object()
        .ok_or_else(|| AppError::validation("record payload must be a JSON object"))?;
    let mut normalized = object.clone();
    if let Some(service) = normalized.remove("service")
        && !normalized.contains_key("services")
    {
        normalized.insert("services".to_string(), service);
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

    if type_name.as_str() == "SRV"
        && !owner_name
            .split('.')
            .take(2)
            .all(|label| label.starts_with('_'))
    {
        return Err(AppError::validation(
            "SRV owner names must start with _service._proto labels",
        ));
    }

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
        "DS" | "CDS" => validate_ds_payload(normalized),
        "DNSKEY" | "CDNSKEY" => validate_dnskey_payload(normalized),
        "SMIMEA" => validate_tlsa_payload(normalized),
        "CAA" => validate_caa_payload(normalized),
        "TLSA" => validate_tlsa_payload(normalized),
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
    for (field, value) in [
        ("size_m", size_m),
        ("horizontal_precision_m", horizontal_precision_m),
        ("vertical_precision_m", vertical_precision_m),
    ] {
        if value <= 0.0 {
            return Err(AppError::validation(format!(
                "LOC {} must be greater than zero",
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

pub(crate) fn validate_ds_payload(normalized: &Value) -> Result<Value, AppError> {
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

    // RFC 8624 recommended algorithms
    if !matches!(algorithm, 8 | 10 | 13 | 14 | 15 | 16) {
        return Err(AppError::validation(
            "DS algorithm must be one of the currently recommended IANA values: 8 (RSASHA256), 10 (RSASHA512), 13 (ECDSAP256SHA256), 14 (ECDSAP384SHA384), 15 (ED25519), 16 (ED448)",
        ));
    }

    // RFC 8624 recommended digest types
    if !matches!(digest_type, 2 | 4) {
        return Err(AppError::validation(
            "DS digest_type must be one of the currently recommended IANA values: 2 (SHA-256), 4 (SHA-384)",
        ));
    }

    let digest = HexEncodedValue::new(
        normalized
            .get("digest")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("DS digest is required"))?,
    )?;

    Ok(json!({
        "key_tag": key_tag,
        "algorithm": algorithm,
        "digest_type": digest_type,
        "digest": digest.as_str(),
    }))
}

pub(crate) fn validate_dnskey_payload(normalized: &Value) -> Result<Value, AppError> {
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

    // RFC 8624 recommended algorithms
    if !matches!(algorithm, 8 | 10 | 13 | 14 | 15 | 16) {
        return Err(AppError::validation(
            "DNSKEY algorithm must be one of the currently recommended IANA values: 8 (RSASHA256), 10 (RSASHA512), 13 (ECDSAP256SHA256), 14 (ECDSAP384SHA384), 15 (ED25519), 16 (ED448)",
        ));
    }

    let public_key = normalized
        .get("public_key")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("DNSKEY public_key is required"))?;

    // public_key should be base64-encoded
    if public_key.is_empty() {
        return Err(AppError::validation("DNSKEY public_key cannot be empty"));
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
    if flags > 255 {
        return Err(AppError::validation("CAA flags must be 0-255"));
    }
    let tag = normalized
        .get("tag")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::validation("CAA tag is required"))?;
    // RFC 8659: tag must be US-ASCII lowercase alphanumeric
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
    if usage > 3 {
        return Err(AppError::validation(
            "TLSA usage must be 0-3 (PKIX-TA, PKIX-EE, DANE-TA, DANE-EE)",
        ));
    }
    if selector > 1 {
        return Err(AppError::validation(
            "TLSA selector must be 0 (Full) or 1 (SubjectPublicKeyInfo)",
        ));
    }
    if matching_type > 2 {
        return Err(AppError::validation(
            "TLSA matching_type must be 0 (Full), 1 (SHA-256), or 2 (SHA-512)",
        ));
    }

    let certificate_data = HexEncodedValue::new(
        normalized
            .get("certificate_data")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::validation("TLSA certificate_data is required"))?,
    )?;

    Ok(json!({
        "usage": usage,
        "selector": selector,
        "matching_type": matching_type,
        "certificate_data": certificate_data.as_str(),
    }))
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
