pub mod resource_kinds {
    pub const SYSTEM: &str = "system";
    pub const AUDIT_HISTORY: &str = "audit_history";
    pub const HOST: &str = "host";
    pub const IP_ADDRESS: &str = "ip_address";
    pub const LABEL: &str = "label";
    pub const NAMESERVER: &str = "nameserver";
    pub const FORWARD_ZONE: &str = "forward_zone";
    pub const REVERSE_ZONE: &str = "reverse_zone";
    pub const FORWARD_ZONE_DELEGATION: &str = "forward_zone_delegation";
    pub const REVERSE_ZONE_DELEGATION: &str = "reverse_zone_delegation";
    pub const NETWORK: &str = "network";
    pub const EXCLUDED_RANGE: &str = "excluded_range";
    pub const RECORD_TYPE: &str = "record_type";
    pub const RECORD: &str = "record";
    pub const RRSET: &str = "rrset";
    pub const HOST_CONTACT: &str = "host_contact";
    pub const HOST_GROUP: &str = "host_group";
    pub const BACNET_ID: &str = "bacnet_id";
    pub const PTR_OVERRIDE: &str = "ptr_override";
    pub const NETWORK_POLICY: &str = "network_policy";
    pub const COMMUNITY: &str = "community";
    pub const HOST_ATTACHMENT: &str = "host_attachment";
    pub const ATTACHMENT_DHCP_IDENTIFIER: &str = "attachment_dhcp_identifier";
    pub const ATTACHMENT_PREFIX_RESERVATION: &str = "attachment_prefix_reservation";
    pub const ATTACHMENT_COMMUNITY_ASSIGNMENT: &str = "attachment_community_assignment";
    pub const HOST_COMMUNITY_ASSIGNMENT: &str = "host_community_assignment";
    pub const HOST_POLICY_ATOM: &str = "host_policy_atom";
    pub const HOST_POLICY_ROLE: &str = "host_policy_role";
    pub const AUTH_SESSION: &str = "auth_session";
    pub const IMPORT_BATCH: &str = "import_batch";
    pub const EXPORT_TEMPLATE: &str = "export_template";
    pub const EXPORT_RUN: &str = "export_run";
    pub const TASK: &str = "task";
}

pub mod system {
    pub const HEALTH_GET: &str = "system.health.get";
    pub const VERSION_GET: &str = "system.version.get";
    pub const STATUS_GET: &str = "system.status.get";
}

pub mod auth_session {
    pub const LOGOUT_ALL: &str = "auth_session.logout_all";
}

pub mod audit {
    pub const HISTORY_LIST: &str = "audit.history.list";
}

pub mod host {
    pub const LIST: &str = "host.list";
    pub const GET: &str = "host.get";
    pub const CREATE: &str = "host.create";
    pub const UPDATE_NAME: &str = "host.update.name";
    pub const UPDATE_ZONE: &str = "host.update.zone";
    pub const UPDATE_TTL: &str = "host.update.ttl";
    pub const UPDATE_COMMENT: &str = "host.update.comment";
    pub const DELETE: &str = "host.delete";

    pub mod ip {
        pub const LIST: &str = "host.ip.list";
        pub const LIST_FOR_HOST: &str = "host.ip.list_for_host";
        pub const ASSIGN_MANUAL: &str = "host.ip.assign_manual";
        pub const ASSIGN_AUTO: &str = "host.ip.assign_auto";
        pub const UPDATE_MAC: &str = "host.ip.update.mac";
        pub const UNASSIGN: &str = "host.ip.unassign";
    }
}

pub mod label {
    pub const LIST: &str = "label.list";
    pub const GET: &str = "label.get";
    pub const CREATE: &str = "label.create";
    pub const UPDATE_DESCRIPTION: &str = "label.update.description";
    pub const DELETE: &str = "label.delete";
}

pub mod nameserver {
    pub const LIST: &str = "nameserver.list";
    pub const GET: &str = "nameserver.get";
    pub const CREATE: &str = "nameserver.create";
    pub const UPDATE_TTL: &str = "nameserver.update.ttl";
    pub const DELETE: &str = "nameserver.delete";
}

pub mod zone {
    pub mod forward {
        pub const LIST: &str = "zone.forward.list";
        pub const GET: &str = "zone.forward.get";
        pub const CREATE: &str = "zone.forward.create";
        pub const UPDATE_PRIMARY_NS: &str = "zone.forward.update.primary_ns";
        pub const UPDATE_NAMESERVERS: &str = "zone.forward.update.nameservers";
        pub const UPDATE_EMAIL: &str = "zone.forward.update.email";
        pub const UPDATE_TIMING: &str = "zone.forward.update.timing";
        pub const DELETE: &str = "zone.forward.delete";

        pub mod delegation {
            pub const LIST: &str = "zone.forward.delegation.list";
            pub const CREATE: &str = "zone.forward.delegation.create";
            pub const DELETE: &str = "zone.forward.delegation.delete";
        }
    }

    pub mod reverse {
        pub const LIST: &str = "zone.reverse.list";
        pub const GET: &str = "zone.reverse.get";
        pub const CREATE: &str = "zone.reverse.create";
        pub const UPDATE_PRIMARY_NS: &str = "zone.reverse.update.primary_ns";
        pub const UPDATE_NAMESERVERS: &str = "zone.reverse.update.nameservers";
        pub const UPDATE_EMAIL: &str = "zone.reverse.update.email";
        pub const UPDATE_TIMING: &str = "zone.reverse.update.timing";
        pub const DELETE: &str = "zone.reverse.delete";

        pub mod delegation {
            pub const LIST: &str = "zone.reverse.delegation.list";
            pub const CREATE: &str = "zone.reverse.delegation.create";
            pub const DELETE: &str = "zone.reverse.delegation.delete";
        }
    }
}

pub mod network {
    pub const LIST: &str = "network.list";
    pub const GET: &str = "network.get";
    pub const CREATE: &str = "network.create";
    pub const UPDATE_DESCRIPTION: &str = "network.update.description";
    pub const UPDATE_VLAN: &str = "network.update.vlan";
    pub const UPDATE_DNS_DELEGATED: &str = "network.update.dns_delegated";
    pub const UPDATE_CATEGORY: &str = "network.update.category";
    pub const UPDATE_LOCATION: &str = "network.update.location";
    pub const UPDATE_FROZEN: &str = "network.update.frozen";
    pub const UPDATE_RESERVED: &str = "network.update.reserved";
    pub const DELETE: &str = "network.delete";
    pub const EXCLUDED_RANGE_LIST: &str = "network.excluded_range.list";
    pub const EXCLUDED_RANGE_CREATE: &str = "network.excluded_range.create";
    pub const ADDRESS_LIST_USED: &str = "network.address.list_used";
    pub const ADDRESS_LIST_UNUSED: &str = "network.address.list_unused";
}

pub mod record_type {
    pub const LIST: &str = "record_type.list";
    pub const CREATE: &str = "record_type.create";
    pub const DELETE: &str = "record_type.delete";
}

pub mod record {
    pub const LIST: &str = "record.list";
    pub const GET: &str = "record.get";
    pub const CREATE_ANCHORED: &str = "record.create.anchored";
    pub const CREATE_UNANCHORED: &str = "record.create.unanchored";
    pub const UPDATE_TTL: &str = "record.update.ttl";
    pub const UPDATE_DATA: &str = "record.update.data";
    pub const DELETE: &str = "record.delete";
}

pub mod rrset {
    pub const LIST: &str = "rrset.list";
    pub const GET: &str = "rrset.get";
    pub const DELETE: &str = "rrset.delete";
}

pub mod host_contact {
    pub const LIST: &str = "host_contact.list";
    pub const GET: &str = "host_contact.get";
    pub const CREATE: &str = "host_contact.create";
    pub const DELETE: &str = "host_contact.delete";
}

pub mod host_group {
    pub const LIST: &str = "host_group.list";
    pub const GET: &str = "host_group.get";
    pub const CREATE: &str = "host_group.create";
    pub const DELETE: &str = "host_group.delete";
}

pub mod bacnet_id {
    pub const LIST: &str = "bacnet_id.list";
    pub const GET: &str = "bacnet_id.get";
    pub const CREATE: &str = "bacnet_id.create";
    pub const DELETE: &str = "bacnet_id.delete";
}

pub mod ptr_override {
    pub const LIST: &str = "ptr_override.list";
    pub const GET: &str = "ptr_override.get";
    pub const CREATE: &str = "ptr_override.create";
    pub const DELETE: &str = "ptr_override.delete";
}

pub mod network_policy {
    pub const LIST: &str = "network_policy.list";
    pub const GET: &str = "network_policy.get";
    pub const CREATE: &str = "network_policy.create";
    pub const DELETE: &str = "network_policy.delete";
}

pub mod community {
    pub const LIST: &str = "community.list";
    pub const GET: &str = "community.get";
    pub const CREATE: &str = "community.create";
    pub const DELETE: &str = "community.delete";
}

pub mod host_attachment {
    pub const LIST: &str = "host_attachment.list";
    pub const GET: &str = "host_attachment.get";
    pub const CREATE: &str = "host_attachment.create";
    pub const UPDATE: &str = "host_attachment.update";
    pub const DELETE: &str = "host_attachment.delete";
}

pub mod attachment_dhcp_identifier {
    pub const LIST: &str = "attachment_dhcp_identifier.list";
    pub const CREATE: &str = "attachment_dhcp_identifier.create";
    pub const DELETE: &str = "attachment_dhcp_identifier.delete";
}

pub mod attachment_prefix_reservation {
    pub const LIST: &str = "attachment_prefix_reservation.list";
    pub const CREATE: &str = "attachment_prefix_reservation.create";
    pub const DELETE: &str = "attachment_prefix_reservation.delete";
}

pub mod attachment_community_assignment {
    pub const LIST: &str = "attachment_community_assignment.list";
    pub const GET: &str = "attachment_community_assignment.get";
    pub const CREATE: &str = "attachment_community_assignment.create";
    pub const DELETE: &str = "attachment_community_assignment.delete";
}

pub mod host_community_assignment {
    pub const LIST: &str = "host_community_assignment.list";
    pub const GET: &str = "host_community_assignment.get";
    pub const CREATE: &str = "host_community_assignment.create";
    pub const DELETE: &str = "host_community_assignment.delete";
}

pub mod host_policy {
    pub mod atom {
        pub const LIST: &str = "host_policy.atom.list";
        pub const GET: &str = "host_policy.atom.get";
        pub const CREATE: &str = "host_policy.atom.create";
        pub const UPDATE_DESCRIPTION: &str = "host_policy.atom.update.description";
        pub const DELETE: &str = "host_policy.atom.delete";
    }

    pub mod role {
        pub const LIST: &str = "host_policy.role.list";
        pub const GET: &str = "host_policy.role.get";
        pub const CREATE: &str = "host_policy.role.create";
        pub const UPDATE_DESCRIPTION: &str = "host_policy.role.update.description";
        pub const DELETE: &str = "host_policy.role.delete";
        pub const ATOM_ATTACH: &str = "host_policy.role.atom.attach";
        pub const ATOM_DETACH: &str = "host_policy.role.atom.detach";
        pub const HOST_ATTACH: &str = "host_policy.role.host.attach";
        pub const HOST_DETACH: &str = "host_policy.role.host.detach";
        pub const LABEL_ATTACH: &str = "host_policy.role.label.attach";
        pub const LABEL_DETACH: &str = "host_policy.role.label.detach";
    }
}

pub mod import_batch {
    pub const LIST: &str = "import.batch.list";
    pub const CREATE: &str = "import.batch.create";
    pub const RUN: &str = "import.batch.run";
}

pub mod export_template {
    pub const LIST: &str = "export.template.list";
    pub const CREATE: &str = "export.template.create";
}

pub mod export_run {
    pub const LIST: &str = "export.run.list";
    pub const CREATE: &str = "export.run.create";
    pub const EXECUTE: &str = "export.run.execute";
}

pub mod task {
    pub const LIST: &str = "task.list";
}

pub mod worker {
    pub const TASK_CLAIM_NEXT: &str = "worker.task.claim_next";
    pub const TASK_EXECUTE_IMPORT_BATCH: &str = "worker.task.execute.import_batch";
    pub const TASK_EXECUTE_EXPORT_RUN: &str = "worker.task.execute.export_run";
}
