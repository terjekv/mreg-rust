use minijinja::Environment;
use serde_json::Value;

use crate::{domain::types::RecordTypeName, errors::AppError};

/// Render validated RDATA in DNS master-file presentation format.
///
/// Built-in types use explicit renderers so domain names and character strings
/// cannot accidentally inherit the surrounding `$ORIGIN` or break quoting.
/// Runtime-defined types retain their validated MiniJinja template.
pub fn render_record_data(
    type_name: &RecordTypeName,
    template: Option<&str>,
    data: &Value,
) -> Result<Option<String>, AppError> {
    let rendered = match type_name.as_str() {
        "A" | "AAAA" => string(data, "address")?.to_string(),
        "NS" => absolute_name(string(data, "nsdname")?),
        "PTR" => absolute_name(string(data, "ptrdname")?),
        "CNAME" | "DNAME" => absolute_name(string(data, "target")?),
        "MX" => format!(
            "{} {}",
            integer(data, "preference")?,
            absolute_name(string(data, "exchange")?)
        ),
        "SRV" => format!(
            "{} {} {} {}",
            integer(data, "priority")?,
            integer(data, "weight")?,
            integer(data, "port")?,
            absolute_name(string(data, "target")?)
        ),
        "TXT" => data
            .get("value")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::internal("validated TXT data is not an array"))?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(quote_character_string)
                    .ok_or_else(|| AppError::internal("validated TXT item is not a string"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(" "),
        "HINFO" => format!(
            "{} {}",
            quote_character_string(string(data, "cpu")?),
            quote_character_string(string(data, "os")?)
        ),
        "NAPTR" => format!(
            "{} {} {} {} {} {}",
            integer(data, "order")?,
            integer(data, "preference")?,
            quote_character_string(string(data, "flags")?),
            quote_character_string(string(data, "services")?),
            quote_character_string(string(data, "regexp")?),
            absolute_name(string(data, "replacement")?)
        ),
        "LOC" => render_loc(data)?,
        "URI" => format!(
            "{} {} {}",
            integer(data, "priority")?,
            integer(data, "weight")?,
            quote_character_string(string(data, "target")?)
        ),
        "CAA" => format!(
            "{} {} {}",
            integer(data, "flags")?,
            quote_character_string(string(data, "tag")?),
            quote_character_string(string(data, "value")?)
        ),
        "SSHFP" => format!(
            "{} {} {}",
            integer(data, "algorithm")?,
            integer(data, "fp_type")?,
            string(data, "fingerprint")?
        ),
        "DS" | "CDS" => format!(
            "{} {} {} {}",
            integer(data, "key_tag")?,
            integer(data, "algorithm")?,
            integer(data, "digest_type")?,
            string(data, "digest")?
        ),
        "DNSKEY" | "CDNSKEY" => format!(
            "{} {} {} {}",
            integer(data, "flags")?,
            integer(data, "protocol")?,
            integer(data, "algorithm")?,
            string(data, "public_key")?
        ),
        "TLSA" | "SMIMEA" => format!(
            "{} {} {} {}",
            integer(data, "usage")?,
            integer(data, "selector")?,
            integer(data, "matching_type")?,
            string(data, "certificate_data")?
        ),
        "OPENPGPKEY" => string(data, "public_key")?.to_string(),
        "CSYNC" => format!(
            "{} {} {}",
            integer(data, "soa_serial")?,
            integer(data, "flags")?,
            string(data, "type_bitmap")?
        ),
        "SVCB" | "HTTPS" => render_svcb(data)?,
        _ => return render_runtime_template(template, data),
    };
    Ok(Some(rendered))
}

pub fn absolute_name(value: &str) -> String {
    if value == "." || value.ends_with('.') {
        value.to_string()
    } else {
        format!("{value}.")
    }
}

pub fn quote_character_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for byte in value.bytes() {
        match byte {
            b'"' | b'\\' => {
                output.push('\\');
                output.push(char::from(byte));
            }
            0x20..=0x7e => output.push(char::from(byte)),
            _ => output.push_str(&format!("\\{byte:03}")),
        }
    }
    output.push('"');
    output
}

pub fn soa_rname(email: &str) -> Result<String, AppError> {
    let (local, domain) = email
        .rsplit_once('@')
        .ok_or_else(|| AppError::internal("validated SOA email is missing '@'"))?;
    let mut escaped_local = String::new();
    for byte in local.bytes() {
        match byte {
            b'.' | b'\\' => {
                escaped_local.push('\\');
                escaped_local.push(char::from(byte));
            }
            0x21..=0x7e => escaped_local.push(char::from(byte)),
            _ => escaped_local.push_str(&format!("\\{byte:03}")),
        }
    }
    Ok(format!("{escaped_local}.{}", absolute_name(domain)))
}

fn render_runtime_template(
    template: Option<&str>,
    data: &Value,
) -> Result<Option<String>, AppError> {
    let Some(template) = template else {
        return Ok(None);
    };
    let mut environment = Environment::new();
    environment
        .add_template("record", template)
        .map_err(AppError::internal)?;
    Ok(Some(
        environment
            .get_template("record")
            .map_err(AppError::internal)?
            .render(minijinja::value::Value::from_serialize(data))
            .map_err(AppError::internal)?,
    ))
}

fn string<'a>(data: &'a Value, field: &str) -> Result<&'a str, AppError> {
    data.get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::internal(format!("validated field '{field}' is not a string")))
}

fn integer(data: &Value, field: &str) -> Result<u64, AppError> {
    data.get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| AppError::internal(format!("validated field '{field}' is not an integer")))
}

fn number(data: &Value, field: &str) -> Result<f64, AppError> {
    data.get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| AppError::internal(format!("validated field '{field}' is not a number")))
}

fn render_loc(data: &Value) -> Result<String, AppError> {
    let latitude = coordinate(number(data, "latitude")?, true);
    let longitude = coordinate(number(data, "longitude")?, false);
    Ok(format!(
        "{} {} {:.2}m {:.2}m {:.2}m {:.2}m",
        latitude,
        longitude,
        number(data, "altitude_m")?,
        number(data, "size_m")?,
        number(data, "horizontal_precision_m")?,
        number(data, "vertical_precision_m")?,
    ))
}

fn coordinate(value: f64, latitude: bool) -> String {
    let absolute = value.abs();
    let degrees = absolute.floor();
    let minutes_value = (absolute - degrees) * 60.0;
    let minutes = minutes_value.floor();
    let seconds = (minutes_value - minutes) * 60.0;
    let hemisphere = match (latitude, value.is_sign_negative()) {
        (true, false) => 'N',
        (true, true) => 'S',
        (false, false) => 'E',
        (false, true) => 'W',
    };
    format!("{degrees:.0} {minutes:.0} {seconds:.3} {hemisphere}")
}

fn render_svcb(data: &Value) -> Result<String, AppError> {
    let mut output = format!(
        "{} {}",
        integer(data, "priority")?,
        absolute_name(string(data, "target")?)
    );
    if let Some(params) = data.get("params").and_then(Value::as_array) {
        for parameter in params {
            let key = string(parameter, "key")?;
            output.push(' ');
            output.push_str(key);
            if let Some(value) = parameter.get("value").and_then(Value::as_str) {
                output.push('=');
                output.push_str(&quote_character_string(value));
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{quote_character_string, render_record_data, soa_rname};
    use crate::domain::types::RecordTypeName;

    #[test]
    fn cname_target_is_absolute() {
        let rendered = render_record_data(
            &RecordTypeName::new("CNAME").unwrap(),
            None,
            &json!({"target": "elsewhere.example"}),
        )
        .unwrap();
        assert_eq!(rendered.as_deref(), Some("elsewhere.example."));
    }

    #[test]
    fn character_string_escapes_master_file_metacharacters() {
        assert_eq!(quote_character_string("a\\\"b\n"), "\"a\\\\\\\"b\\010\"");
    }

    #[test]
    fn soa_rname_escapes_local_part_dots() {
        assert_eq!(
            soa_rname("first.last@example.org").unwrap(),
            "first\\.last.example.org."
        );
    }
}
