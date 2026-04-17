mod aliases;
mod core_names;
mod mail;
mod misc;
mod security;
mod text_service;
mod tls_crypto;

use crate::{domain::resource_records::CreateRecordTypeDefinition, errors::AppError};

/// Returns definitions for all 25 built-in DNS record types.
pub fn built_in_record_types() -> Result<Vec<CreateRecordTypeDefinition>, AppError> {
    Ok(vec![
        core_names::builtin_a()?,
        core_names::builtin_aaaa()?,
        core_names::builtin_ns()?,
        core_names::builtin_ptr()?,
        aliases::builtin_cname()?,
        aliases::builtin_dname()?,
        mail::builtin_mx()?,
        text_service::builtin_txt()?,
        text_service::builtin_srv()?,
        text_service::builtin_naptr()?,
        security::builtin_sshfp()?,
        misc::builtin_loc()?,
        text_service::builtin_hinfo()?,
        security::builtin_ds()?,
        security::builtin_dnskey()?,
        security::builtin_cds()?,
        security::builtin_cdnskey()?,
        security::builtin_csync()?,
        security::builtin_caa()?,
        tls_crypto::builtin_tlsa()?,
        tls_crypto::builtin_svcb()?,
        tls_crypto::builtin_https()?,
        misc::builtin_uri()?,
        tls_crypto::builtin_openpgpkey()?,
        tls_crypto::builtin_smimea()?,
    ])
}
