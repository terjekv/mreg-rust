use actix_web::web;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod v1;
pub mod v2;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "mreg DNS Management API",
        version = "2.0.0",
        description = "Version 2 of the mreg-rust REST API for managing DNS zones, hosts, records, and related resources."
    ),
    paths(
        // System
        v2::system::health,
        v2::system::version,
        v2::system::status,
        v2::system::history,
        // Authentication
        v2::auth::login,
        v2::auth::providers,
        v2::auth::me,
        v2::auth::logout,
        v2::auth::logout_all,
        // Workflows – list endpoints
        v2::workflows::tasks,
        v2::workflows::imports,
        v2::workflows::export_templates,
        v2::workflows::export_runs,
        // DNS – list endpoints
        v2::dns::record_types,
        v2::dns::rrsets,
        v2::dns::list_records_endpoint,
        // Policy – Host policy atoms
        v2::host_policy::list_atoms,
        v2::host_policy::create_atom,
        v2::host_policy::get_atom,
        v2::host_policy::update_atom,
        v2::host_policy::delete_atom,
        // Policy – Host policy roles
        v2::host_policy::list_roles,
        v2::host_policy::create_role,
        v2::host_policy::get_role,
        v2::host_policy::update_role,
        v2::host_policy::delete_role,
        // Policy – Host policy role membership
        v2::host_policy::add_atom_to_role,
        v2::host_policy::remove_atom_from_role,
        v2::host_policy::add_host_to_role,
        v2::host_policy::remove_host_from_role,
        v2::host_policy::add_label_to_role,
        v2::host_policy::remove_label_from_role,
        // Inventory – Labels
        v2::labels::list_labels,
        v2::labels::create_label,
        v2::labels::get_label,
        v2::labels::update_label,
        v2::labels::delete_label,
        // DNS – Nameservers
        v2::nameservers::list_nameservers,
        v2::nameservers::create_nameserver,
        v2::nameservers::get_nameserver,
        v2::nameservers::update_nameserver,
        v2::nameservers::delete_nameserver,
        // Inventory – Hosts
        v2::hosts::list_hosts,
        v2::hosts::create_host,
        v2::hosts::get_host,
        v2::hosts::update_host,
        v2::hosts::delete_host,
        v2::hosts::list_ip_addresses,
        v2::hosts::list_host_ip_addresses,
        v2::hosts::assign_ip_address,
        v2::hosts::unassign_ip_address,
        // DNS – Forward zones
        v2::zones::forward::list_forward_zones,
        v2::zones::forward::create_forward_zone,
        v2::zones::forward::get_forward_zone,
        v2::zones::forward::update_forward_zone,
        v2::zones::forward::delete_forward_zone,
        // DNS – Reverse zones
        v2::zones::reverse::list_reverse_zones,
        v2::zones::reverse::create_reverse_zone,
        v2::zones::reverse::get_reverse_zone,
        v2::zones::reverse::update_reverse_zone,
        v2::zones::reverse::delete_reverse_zone,
        // DNS – Delegations
        v2::zones::delegations::list_forward_zone_delegations,
        v2::zones::delegations::create_forward_zone_delegation,
        v2::zones::delegations::delete_forward_zone_delegation,
        v2::zones::delegations::list_reverse_zone_delegations,
        v2::zones::delegations::create_reverse_zone_delegation,
        v2::zones::delegations::delete_reverse_zone_delegation,
        // Inventory – Networks
        v2::networks::list_networks,
        v2::networks::create_network,
        v2::networks::get_network,
        v2::networks::delete_network,
        v2::networks::list_excluded_ranges,
        v2::networks::create_excluded_range,
        // DNS – Records
        v2::records::create_record_type,
        v2::records::create_record,
        v2::records::get_record_endpoint,
        v2::records::get_rrset_endpoint,
        v2::records::update_record_endpoint,
        v2::records::delete_record_endpoint,
        v2::records::delete_record_type_endpoint,
        v2::records::delete_rrset_endpoint,
        // Inventory – Host contacts
        v2::host_contacts::list_host_contacts,
        v2::host_contacts::create_host_contact,
        v2::host_contacts::get_host_contact,
        v2::host_contacts::delete_host_contact,
        // Inventory – Host groups
        v2::host_groups::list_host_groups,
        v2::host_groups::create_host_group,
        v2::host_groups::get_host_group,
        v2::host_groups::delete_host_group,
        // Inventory – BACnet IDs
        v2::bacnet_ids::list_bacnet_ids,
        v2::bacnet_ids::create_bacnet_id,
        v2::bacnet_ids::get_bacnet_id,
        v2::bacnet_ids::delete_bacnet_id,
        // DNS – PTR overrides
        v2::ptr_overrides::list_ptr_overrides,
        v2::ptr_overrides::create_ptr_override,
        v2::ptr_overrides::get_ptr_override,
        v2::ptr_overrides::delete_ptr_override,
        // Policy – Network policies
        v2::network_policies::list_network_policies,
        v2::network_policies::create_network_policy,
        v2::network_policies::get_network_policy,
        v2::network_policies::update_network_policy,
        v2::network_policies::delete_network_policy,
        v2::network_policies::list_network_policy_attributes,
        v2::network_policies::create_network_policy_attribute,
        v2::network_policies::get_network_policy_attribute,
        v2::network_policies::update_network_policy_attribute,
        v2::network_policies::delete_network_policy_attribute,
        // Policy – Communities
        v2::communities::list_communities,
        v2::communities::create_community,
        v2::communities::get_community,
        v2::communities::delete_community,
        // Policy – Host community assignments
        v2::host_community_assignments::list_host_community_assignments,
        v2::host_community_assignments::create_host_community_assignment,
        v2::host_community_assignments::get_host_community_assignment,
        v2::host_community_assignments::delete_host_community_assignment,
        // Workflows
        v2::workflows::create_import,
        v2::workflows::create_export_template,
        v2::workflows::create_export_run,
        v2::workflows::run_next_task,
    ),
    components(schemas(
        // Pagination
        crate::domain::pagination::SortDirection,
        // Domain types
        crate::domain::resource_records::RecordOwnerKind,
        crate::domain::resource_records::RecordCardinality,
        crate::domain::resource_records::RecordFieldKind,
        crate::domain::resource_records::RecordOwnerNameSyntax,
        crate::domain::resource_records::RecordRfcProfile,
        // Storage types
        crate::storage::StorageBackendKind,
        crate::storage::StorageCapabilities,
        crate::storage::StorageHealthReport,
        // System
        v2::system::HealthResponse,
        v2::system::VersionResponse,
        v2::system::StatusResponse,
        // Authentication
        v2::auth::AuthProvidersResponse,
        v2::auth::LoginRequest,
        v2::auth::LoginResponse,
        v2::auth::MeResponse,
        v2::auth::LogoutAllRequest,
        v2::auth::PrincipalResponse,
        // Host Policy
        v2::host_policy::CreateAtomRequest,
        v2::host_policy::UpdateAtomRequest,
        v2::host_policy::AtomResponse,
        v2::host_policy::CreateRoleRequest,
        v2::host_policy::UpdateRoleRequest,
        v2::host_policy::RoleResponse,
        // Labels
        v2::labels::CreateLabelRequest,
        v2::labels::UpdateLabelRequest,
        v2::labels::LabelResponse,
        // Nameservers
        v2::nameservers::CreateNameServerRequest,
        v2::nameservers::UpdateNameServerRequest,
        v2::nameservers::NameServerResponse,
        // Hosts
        v2::hosts::CreateHostRequest,
        v2::hosts::UpdateHostRequest,
        v2::hosts::HostResponse,
        v2::hosts::AssignIpAddressRequest,
        v2::hosts::IpAddressResponse,
        // Zones – Forward
        v2::zones::forward::CreateForwardZoneRequest,
        v2::zones::forward::UpdateForwardZoneRequest,
        v2::zones::forward::ForwardZoneResponse,
        // Zones – Reverse
        v2::zones::reverse::CreateReverseZoneRequest,
        v2::zones::reverse::UpdateReverseZoneRequest,
        v2::zones::reverse::ReverseZoneResponse,
        // Zones – Delegations
        v2::zones::delegations::CreateDelegationRequest,
        v2::zones::delegations::ForwardZoneDelegationResponse,
        v2::zones::delegations::ReverseZoneDelegationResponse,
        // Networks
        v2::networks::CreateNetworkRequest,
        v2::networks::CreateExcludedRangeRequest,
        v2::networks::NetworkResponse,
        v2::networks::ExcludedRangeResponse,
        // Records
        v2::records::CreateRecordTypeRequest,
        v2::records::CreateRecordFieldSchemaRequest,
        v2::records::CreateRecordRequest,
        v2::records::UpdateRecordRequest,
        v2::records::RecordTypeResponse,
        v2::records::RecordResponse,
        // Typed envelopes for the polymorphic `data` field on RecordResponse.
        v2::records::typed_data::RecordKind,
        v2::records::typed_data::TypedRecordKind,
        v2::records::typed_data::OpaqueRecordKind,
        v2::records::typed_data::ARecordData,
        v2::records::typed_data::AaaaRecordData,
        v2::records::typed_data::NsRecordData,
        v2::records::typed_data::PtrRecordData,
        v2::records::typed_data::CnameRecordData,
        v2::records::typed_data::DnameRecordData,
        v2::records::typed_data::MxRecordData,
        v2::records::typed_data::TxtRecordData,
        v2::records::typed_data::SrvRecordData,
        v2::records::typed_data::NaptrRecordData,
        v2::records::typed_data::SshfpRecordData,
        v2::records::typed_data::LocRecordData,
        v2::records::typed_data::HinfoRecordData,
        v2::records::typed_data::DsRecordData,
        v2::records::typed_data::DnskeyRecordData,
        v2::records::typed_data::CdsRecordData,
        v2::records::typed_data::CdnskeyRecordData,
        v2::records::typed_data::CsyncRecordData,
        v2::records::typed_data::CaaRecordData,
        v2::records::typed_data::TlsaRecordData,
        v2::records::typed_data::SmimeaRecordData,
        v2::records::typed_data::SvcbRecordData,
        v2::records::typed_data::HttpsRecordData,
        v2::records::typed_data::UriRecordData,
        v2::records::typed_data::OpenpgpkeyRecordData,
        // Host contacts
        v2::host_contacts::CreateHostContactRequest,
        v2::host_contacts::HostContactResponse,
        v2::host_groups::CreateHostGroupRequest,
        v2::host_groups::HostGroupResponse,
        v2::bacnet_ids::CreateBacnetRequest,
        v2::bacnet_ids::BacnetResponse,
        v2::ptr_overrides::CreatePtrOverrideRequest,
        v2::ptr_overrides::PtrOverrideResponse,
        v2::network_policies::CreateNetworkPolicyRequest,
        v2::network_policies::NetworkPolicyResponse,
        v2::network_policies::NetworkPolicyAttributeValueRequest,
        v2::network_policies::NetworkPolicyAttributeValueResponse,
        v2::network_policies::CreateNetworkPolicyAttributeRequest,
        v2::network_policies::UpdateNetworkPolicyAttributeRequest,
        v2::network_policies::NetworkPolicyAttributeResponse,
        v2::network_policies::NetworkPolicyAttributePageResponse,
        v2::communities::CreateCommunityRequest,
        v2::communities::CommunityResponse,
        v2::host_community_assignments::CreateHostCommunityAssignmentRequest,
        v2::host_community_assignments::HostCommunityAssignmentResponse,
        // Workflows
        v2::workflows::CreateImportRequest,
        v2::workflows::CreateImportItemRequest,
        crate::domain::imports::ImportKind,
        crate::domain::imports::ImportOperation,
        v2::workflows::CreateExportTemplateRequest,
        v2::workflows::CreateExportRunRequest,
    )),
    tags(
        (name = "Authentication", description = "Login and current-principal endpoints"),
        (name = "System", description = "Health, version, status, and audit endpoints"),
        (name = "DNS", description = "DNS zones, nameservers, delegations, records, and PTR overrides"),
        (name = "Inventory", description = "Hosts, networks, labels, contacts, groups, and BACnet assignments"),
        (name = "Policy", description = "Network policy, communities, host-community assignments, and host policy management"),
        (name = "Workflows", description = "Import, export, and task workflows"),
    )
)]
pub struct V2ApiDoc;

/// Kept as a source-compatible name for users which generated the pre-v2 document in-process.
pub type ApiDoc = V2ApiDoc;

pub fn json_config(limit_bytes: usize) -> web::JsonConfig {
    web::JsonConfig::default().limit(limit_bytes)
}

pub fn configure(cfg: &mut web::ServiceConfig, trust_proxy_headers: bool) {
    let openapi = V2ApiDoc::openapi();
    cfg.configure(v1::configure_unversioned)
        .route(
            "/docs",
            web::get().to(|| async {
                actix_web::HttpResponse::PermanentRedirect()
                    .insert_header((actix_web::http::header::LOCATION, "/docs/"))
                    .finish()
            }),
        )
        .service(v1::scope(trust_proxy_headers))
        .service(
            web::scope("/api/v2").configure(move |cfg| v2::configure(cfg, trust_proxy_headers)),
        )
        .service(SwaggerUi::new("/docs/{_:.*}").url("/docs/schema", openapi.clone()))
        .service(SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", openapi));
}

#[cfg(test)]
mod tests {
    use serde_json::Value;
    use utoipa::OpenApi;

    use super::ApiDoc;

    #[test]
    fn authentication_openapi_exposes_provider_discovery_and_string_scope() {
        let document = serde_json::to_value(ApiDoc::openapi()).unwrap();
        let actual = (
            document
                .pointer("/components/schemas/LoginRequest/properties/identity_scope/type")
                .and_then(Value::as_str),
            document
                .pointer("/paths/~1api~1v2~1auth~1providers/get")
                .is_some(),
        );

        assert_eq!(actual, (Some("string"), true));
    }
}
