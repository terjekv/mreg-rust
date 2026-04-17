use actix_web::web;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

pub mod v1;

#[derive(OpenApi)]
#[openapi(
    info(
        title = "mreg DNS Management API",
        version = "1.0.0",
        description = "REST API for managing DNS zones, hosts, records, and related resources."
    ),
    paths(
        // System
        v1::system::health,
        v1::system::version,
        v1::system::status,
        v1::system::history,
        // Authentication
        v1::auth::login,
        v1::auth::me,
        v1::auth::logout,
        v1::auth::logout_all,
        // Workflows – list endpoints
        v1::workflows::tasks,
        v1::workflows::imports,
        v1::workflows::export_templates,
        v1::workflows::export_runs,
        // DNS – list endpoints
        v1::dns::record_types,
        v1::dns::rrsets,
        v1::dns::list_records_endpoint,
        // Policy – Host policy atoms
        v1::host_policy::list_atoms,
        v1::host_policy::create_atom,
        v1::host_policy::get_atom,
        v1::host_policy::update_atom,
        v1::host_policy::delete_atom,
        // Policy – Host policy roles
        v1::host_policy::list_roles,
        v1::host_policy::create_role,
        v1::host_policy::get_role,
        v1::host_policy::update_role,
        v1::host_policy::delete_role,
        // Policy – Host policy role membership
        v1::host_policy::add_atom_to_role,
        v1::host_policy::remove_atom_from_role,
        v1::host_policy::add_host_to_role,
        v1::host_policy::remove_host_from_role,
        v1::host_policy::add_label_to_role,
        v1::host_policy::remove_label_from_role,
        // Inventory – Labels
        v1::labels::list_labels,
        v1::labels::create_label,
        v1::labels::get_label,
        v1::labels::update_label,
        v1::labels::delete_label,
        // DNS – Nameservers
        v1::nameservers::list_nameservers,
        v1::nameservers::create_nameserver,
        v1::nameservers::get_nameserver,
        v1::nameservers::update_nameserver,
        v1::nameservers::delete_nameserver,
        // Inventory – Hosts
        v1::hosts::list_hosts,
        v1::hosts::create_host,
        v1::hosts::get_host,
        v1::hosts::update_host,
        v1::hosts::delete_host,
        v1::hosts::list_ip_addresses,
        v1::hosts::list_host_ip_addresses,
        v1::hosts::assign_ip_address,
        v1::hosts::unassign_ip_address,
        // DNS – Forward zones
        v1::zones::forward::list_forward_zones,
        v1::zones::forward::create_forward_zone,
        v1::zones::forward::get_forward_zone,
        v1::zones::forward::update_forward_zone,
        v1::zones::forward::delete_forward_zone,
        // DNS – Reverse zones
        v1::zones::reverse::list_reverse_zones,
        v1::zones::reverse::create_reverse_zone,
        v1::zones::reverse::get_reverse_zone,
        v1::zones::reverse::update_reverse_zone,
        v1::zones::reverse::delete_reverse_zone,
        // DNS – Delegations
        v1::zones::delegations::list_forward_zone_delegations,
        v1::zones::delegations::create_forward_zone_delegation,
        v1::zones::delegations::delete_forward_zone_delegation,
        v1::zones::delegations::list_reverse_zone_delegations,
        v1::zones::delegations::create_reverse_zone_delegation,
        v1::zones::delegations::delete_reverse_zone_delegation,
        // Inventory – Networks
        v1::networks::list_networks,
        v1::networks::create_network,
        v1::networks::get_network,
        v1::networks::delete_network,
        v1::networks::list_excluded_ranges,
        v1::networks::create_excluded_range,
        // DNS – Records
        v1::records::create_record_type,
        v1::records::create_record,
        v1::records::get_record_endpoint,
        v1::records::get_rrset_endpoint,
        v1::records::update_record_endpoint,
        v1::records::delete_record_endpoint,
        v1::records::delete_record_type_endpoint,
        v1::records::delete_rrset_endpoint,
        // Inventory – Host contacts
        v1::host_contacts::list_host_contacts,
        v1::host_contacts::create_host_contact,
        v1::host_contacts::get_host_contact,
        v1::host_contacts::delete_host_contact,
        // Inventory – Host groups
        v1::host_groups::list_host_groups,
        v1::host_groups::create_host_group,
        v1::host_groups::get_host_group,
        v1::host_groups::delete_host_group,
        // Inventory – BACnet IDs
        v1::bacnet_ids::list_bacnet_ids,
        v1::bacnet_ids::create_bacnet_id,
        v1::bacnet_ids::get_bacnet_id,
        v1::bacnet_ids::delete_bacnet_id,
        // DNS – PTR overrides
        v1::ptr_overrides::list_ptr_overrides,
        v1::ptr_overrides::create_ptr_override,
        v1::ptr_overrides::get_ptr_override,
        v1::ptr_overrides::delete_ptr_override,
        // Policy – Network policies
        v1::network_policies::list_network_policies,
        v1::network_policies::create_network_policy,
        v1::network_policies::get_network_policy,
        v1::network_policies::delete_network_policy,
        // Policy – Communities
        v1::communities::list_communities,
        v1::communities::create_community,
        v1::communities::get_community,
        v1::communities::delete_community,
        // Policy – Host community assignments
        v1::host_community_assignments::list_host_community_assignments,
        v1::host_community_assignments::create_host_community_assignment,
        v1::host_community_assignments::get_host_community_assignment,
        v1::host_community_assignments::delete_host_community_assignment,
        // Workflows
        v1::workflows::create_import,
        v1::workflows::create_export_template,
        v1::workflows::create_export_run,
        v1::workflows::run_next_task,
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
        v1::system::HealthResponse,
        v1::system::VersionResponse,
        v1::system::StatusResponse,
        // Authentication
        v1::auth::LoginRequest,
        v1::auth::LoginResponse,
        v1::auth::MeResponse,
        v1::auth::LogoutAllRequest,
        v1::auth::PrincipalResponse,
        // Host Policy
        v1::host_policy::CreateAtomRequest,
        v1::host_policy::UpdateAtomRequest,
        v1::host_policy::AtomResponse,
        v1::host_policy::CreateRoleRequest,
        v1::host_policy::UpdateRoleRequest,
        v1::host_policy::RoleResponse,
        // Labels
        v1::labels::CreateLabelRequest,
        v1::labels::UpdateLabelRequest,
        v1::labels::LabelResponse,
        // Nameservers
        v1::nameservers::CreateNameServerRequest,
        v1::nameservers::UpdateNameServerRequest,
        v1::nameservers::NameServerResponse,
        // Hosts
        v1::hosts::CreateHostRequest,
        v1::hosts::UpdateHostRequest,
        v1::hosts::HostResponse,
        v1::hosts::AssignIpAddressRequest,
        v1::hosts::IpAddressResponse,
        // Zones – Forward
        v1::zones::forward::CreateForwardZoneRequest,
        v1::zones::forward::UpdateForwardZoneRequest,
        v1::zones::forward::ForwardZoneResponse,
        // Zones – Reverse
        v1::zones::reverse::CreateReverseZoneRequest,
        v1::zones::reverse::UpdateReverseZoneRequest,
        v1::zones::reverse::ReverseZoneResponse,
        // Zones – Delegations
        v1::zones::delegations::CreateDelegationRequest,
        v1::zones::delegations::ForwardZoneDelegationResponse,
        v1::zones::delegations::ReverseZoneDelegationResponse,
        // Networks
        v1::networks::CreateNetworkRequest,
        v1::networks::CreateExcludedRangeRequest,
        v1::networks::NetworkResponse,
        v1::networks::ExcludedRangeResponse,
        // Records
        v1::records::CreateRecordTypeRequest,
        v1::records::CreateRecordFieldSchemaRequest,
        v1::records::CreateRecordRequest,
        v1::records::UpdateRecordRequest,
        v1::records::RecordTypeResponse,
        v1::records::RecordResponse,
        // Host contacts
        v1::host_contacts::CreateHostContactRequest,
        v1::host_contacts::HostContactResponse,
        v1::host_groups::CreateHostGroupRequest,
        v1::host_groups::HostGroupResponse,
        v1::bacnet_ids::CreateBacnetRequest,
        v1::bacnet_ids::BacnetResponse,
        v1::ptr_overrides::CreatePtrOverrideRequest,
        v1::ptr_overrides::PtrOverrideResponse,
        v1::network_policies::CreateNetworkPolicyRequest,
        v1::network_policies::NetworkPolicyResponse,
        v1::communities::CreateCommunityRequest,
        v1::communities::CommunityResponse,
        v1::host_community_assignments::CreateHostCommunityAssignmentRequest,
        v1::host_community_assignments::HostCommunityAssignmentResponse,
        // Workflows
        v1::workflows::CreateImportRequest,
        v1::workflows::CreateImportItemRequest,
        crate::domain::imports::ImportKind,
        crate::domain::imports::ImportOperation,
        v1::workflows::CreateExportTemplateRequest,
        v1::workflows::CreateExportRunRequest,
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
pub struct ApiDoc;

pub fn json_config(limit_bytes: usize) -> web::JsonConfig {
    web::JsonConfig::default().limit(limit_bytes)
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(web::scope("/api/v1").configure(v1::configure))
        .service(
            SwaggerUi::new("/swagger-ui/{_:.*}").url("/api-docs/openapi.json", ApiDoc::openapi()),
        );
}
