@id("mreg.admins.allow_all")
permit (
    principal in Group::"mreg-admins",
    action,
    resource
);

@id("mreg.readonly.reads")
permit (
    principal in Group::"mreg-readonly",
    action in [
        Action::"system.health.get",
        Action::"system.version.get",
        Action::"system.status.get",
        Action::"audit.history.list",
        Action::"task.list",
        Action::"import.batch.list",
        Action::"export.template.list",
        Action::"export.run.list",
        Action::"host.list",
        Action::"host.get",
        Action::"host.ip.list",
        Action::"host.ip.list_for_host",
        Action::"label.list",
        Action::"label.get",
        Action::"nameserver.list",
        Action::"nameserver.get",
        Action::"zone.forward.list",
        Action::"zone.forward.get",
        Action::"zone.forward.delegation.list",
        Action::"zone.reverse.list",
        Action::"zone.reverse.get",
        Action::"zone.reverse.delegation.list",
        Action::"network.list",
        Action::"network.get",
        Action::"network.excluded_range.list",
        Action::"network.address.list_used",
        Action::"network.address.list_unused",
        Action::"record_type.list",
        Action::"record.list",
        Action::"record.get",
        Action::"rrset.list",
        Action::"rrset.get",
        Action::"host_contact.list",
        Action::"host_contact.get",
        Action::"host_group.list",
        Action::"bacnet_id.list",
        Action::"ptr_override.list",
        Action::"network_policy.list",
        Action::"community.list",
        Action::"host_community_assignment.list",
        Action::"host_policy.atom.list",
        Action::"host_policy.atom.get",
        Action::"host_policy.role.list",
        Action::"host_policy.role.get"
    ],
    resource
);

@id("mreg.host_operators")
permit (
    principal in Group::"mreg-host-operators",
    action in [
        Action::"host.list",
        Action::"host.get",
        Action::"host.create",
        Action::"host.delete",
        Action::"host.ip.list",
        Action::"host.ip.list_for_host",
        Action::"host.ip.assign_manual",
        Action::"host.ip.assign_auto",
        Action::"host.ip.update.mac",
        Action::"host.ip.unassign"
    ],
    resource
);

@id("mreg.host_commenters")
permit (
    principal in Group::"mreg-host-commenters",
    action in [
        Action::"host.list",
        Action::"host.get",
        Action::"host.update.comment"
    ],
    resource
);

@id("mreg.host_network_commenters")
permit (
    principal in Group::"mreg-host-network-commenters",
    action in [
        Action::"host.get",
        Action::"host.update.comment"
    ],
    resource
) when {
    resource has networks &&
    resource.networks.contains("10.250.1.0/24")
};

@id("mreg.host_zone_editors")
permit (
    principal in Group::"mreg-host-zone-editors",
    action in [
        Action::"host.list",
        Action::"host.get",
        Action::"host.update.zone"
    ],
    resource
);

@id("mreg.label_managers")
permit (
    principal in Group::"mreg-label-managers",
    action in [
        Action::"label.list",
        Action::"label.get",
        Action::"label.create",
        Action::"label.update.description",
        Action::"label.delete"
    ],
    resource
);

@id("mreg.nameserver_operators")
permit (
    principal in Group::"mreg-nameserver-operators",
    action in [
        Action::"nameserver.list",
        Action::"nameserver.get",
        Action::"nameserver.create",
        Action::"nameserver.update.ttl",
        Action::"nameserver.delete"
    ],
    resource
);

@id("mreg.network_operators")
permit (
    principal in Group::"mreg-network-operators",
    action in [
        Action::"network.list",
        Action::"network.get",
        Action::"network.create",
        Action::"network.delete",
        Action::"network.excluded_range.list",
        Action::"network.excluded_range.create",
        Action::"network.address.list_used",
        Action::"network.address.list_unused"
    ],
    resource
);

@id("mreg.network_reserved_editors")
permit (
    principal in Group::"mreg-network-reserved-editors",
    action in [
        Action::"network.list",
        Action::"network.get",
        Action::"network.update.reserved"
    ],
    resource
);

@id("mreg.zone_operators")
permit (
    principal in Group::"mreg-zone-operators",
    action in [
        Action::"zone.forward.list",
        Action::"zone.forward.get",
        Action::"zone.forward.create",
        Action::"zone.forward.delete",
        Action::"zone.forward.delegation.list",
        Action::"zone.forward.delegation.create",
        Action::"zone.forward.delegation.delete",
        Action::"zone.reverse.list",
        Action::"zone.reverse.get",
        Action::"zone.reverse.create",
        Action::"zone.reverse.delete",
        Action::"zone.reverse.delegation.list",
        Action::"zone.reverse.delegation.create",
        Action::"zone.reverse.delegation.delete"
    ],
    resource
);

@id("mreg.zone_timing_editors")
permit (
    principal in Group::"mreg-zone-timing-editors",
    action in [
        Action::"zone.forward.list",
        Action::"zone.forward.get",
        Action::"zone.forward.update.timing",
        Action::"zone.reverse.list",
        Action::"zone.reverse.get",
        Action::"zone.reverse.update.timing"
    ],
    resource
);

@id("mreg.record_operators")
permit (
    principal in Group::"mreg-record-operators",
    action in [
        Action::"record_type.list",
        Action::"record_type.create",
        Action::"record_type.delete",
        Action::"record.list",
        Action::"record.get",
        Action::"record.create.anchored",
        Action::"record.create.unanchored",
        Action::"record.delete",
        Action::"rrset.list",
        Action::"rrset.get",
        Action::"rrset.delete"
    ],
    resource
);

@id("mreg.record_ttl_editors")
permit (
    principal in Group::"mreg-record-ttl-editors",
    action in [
        Action::"record.list",
        Action::"record.get",
        Action::"record.update.ttl"
    ],
    resource
);

@id("mreg.record_data_editors")
permit (
    principal in Group::"mreg-record-data-editors",
    action in [
        Action::"record.list",
        Action::"record.get",
        Action::"record.update.data"
    ],
    resource
);

@id("mreg.host_contact_managers")
permit (
    principal in Group::"mreg-host-contact-managers",
    action in [
        Action::"host_contact.list",
        Action::"host_contact.get",
        Action::"host_contact.create",
        Action::"host_contact.delete"
    ],
    resource
);

@id("mreg.host_policy_managers")
permit (
    principal in Group::"mreg-host-policy-managers",
    action in [
        Action::"host_policy.atom.list",
        Action::"host_policy.atom.get",
        Action::"host_policy.atom.create",
        Action::"host_policy.atom.update.description",
        Action::"host_policy.atom.delete",
        Action::"host_policy.role.list",
        Action::"host_policy.role.get",
        Action::"host_policy.role.create",
        Action::"host_policy.role.update.description",
        Action::"host_policy.role.delete",
        Action::"host_policy.role.atom.attach",
        Action::"host_policy.role.atom.detach",
        Action::"host_policy.role.host.attach",
        Action::"host_policy.role.host.detach",
        Action::"host_policy.role.label.attach",
        Action::"host_policy.role.label.detach"
    ],
    resource
);

@id("mreg.workflow_creators")
permit (
    principal in Group::"mreg-workflow-creators",
    action in [
        Action::"task.list",
        Action::"import.batch.list",
        Action::"import.batch.create",
        Action::"export.template.list",
        Action::"export.template.create",
        Action::"export.run.list",
        Action::"export.run.create"
    ],
    resource
);

@id("mreg.workers.task_claim_and_import")
permit (
    principal in Group::"mreg-workers",
    action in [
        Action::"task.list",
        Action::"worker.task.claim_next",
        Action::"worker.task.execute.import_batch"
    ],
    resource
);

@id("mreg.workers.no_export_execution")
forbid (
    principal in Group::"mreg-workers",
    action == Action::"worker.task.execute.export_run",
    resource
);
