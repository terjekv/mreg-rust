use actix_web::{HttpRequest, HttpResponse, delete, get, patch, post, web};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    AppState,
    authz::{self, AttrValue, require_permission},
    domain::{
        attachment::{
            AttachmentDhcpIdentifier, AttachmentPrefixReservation, CreateAttachmentDhcpIdentifier,
            CreateAttachmentPrefixReservation, CreateHostAttachment, DhcpIdentifierFamily,
            DhcpIdentifierKind, HostAttachment, UpdateHostAttachment,
        },
        host::AssignIpAddress,
        types::{CidrValue, DhcpPriority, Hostname, IpAddressValue, MacAddressValue, UpdateField},
    },
    errors::AppError,
};

use super::authz::request as authz_request;
use super::hosts::IpAddressResponse;

crate::page_response!(
    HostAttachmentPageResponse,
    HostAttachmentResponse,
    "Paginated list of host attachments."
);

#[derive(Serialize, ToSchema)]
pub struct HostAttachmentResponse {
    pub id: Uuid,
    pub host_id: Uuid,
    pub host_name: String,
    pub network_id: Uuid,
    pub network: String,
    pub mac_address: Option<String>,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl HostAttachmentResponse {
    pub fn from_domain(value: &HostAttachment) -> Self {
        Self {
            id: value.id(),
            host_id: value.host_id(),
            host_name: value.host_name().as_str().to_string(),
            network_id: value.network_id(),
            network: value.network_cidr().as_str(),
            mac_address: value.mac_address().map(|v| v.as_str()),
            comment: value.comment().map(str::to_string),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub struct AttachmentDhcpIdentifierResponse {
    id: Uuid,
    attachment_id: Uuid,
    family: u8,
    kind: String,
    value: String,
    priority: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentDhcpIdentifierResponse {
    pub fn from_domain(value: &AttachmentDhcpIdentifier) -> Self {
        let kind = match value.kind() {
            DhcpIdentifierKind::ClientId => "client_id",
            DhcpIdentifierKind::DuidLlt => "duid_llt",
            DhcpIdentifierKind::DuidEn => "duid_en",
            DhcpIdentifierKind::DuidLl => "duid_ll",
            DhcpIdentifierKind::DuidUuid => "duid_uuid",
            DhcpIdentifierKind::DuidRaw => "duid_raw",
        };
        Self {
            id: value.id(),
            attachment_id: value.attachment_id(),
            family: value.family().as_u8(),
            kind: kind.to_string(),
            value: value.value().to_string(),
            priority: value.priority().as_i32(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Clone, Serialize, ToSchema)]
pub struct AttachmentPrefixReservationResponse {
    id: Uuid,
    attachment_id: Uuid,
    prefix: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AttachmentPrefixReservationResponse {
    pub fn from_domain(value: &AttachmentPrefixReservation) -> Self {
        Self {
            id: value.id(),
            attachment_id: value.attachment_id(),
            prefix: value.prefix().as_str(),
            created_at: value.created_at(),
            updated_at: value.updated_at(),
        }
    }
}

#[derive(Serialize, ToSchema)]
pub struct HostAttachmentDetailResponse {
    #[serde(flatten)]
    attachment: HostAttachmentResponse,
    ip_addresses: Vec<IpAddressResponse>,
    dhcp_identifiers: Vec<AttachmentDhcpIdentifierResponse>,
    prefix_reservations: Vec<AttachmentPrefixReservationResponse>,
}

#[derive(Deserialize, ToSchema)]
pub struct CreateHostAttachmentRequest {
    network: String,
    mac_address: Option<String>,
    comment: Option<String>,
}

impl CreateHostAttachmentRequest {
    fn into_command(self, host_name: Hostname) -> Result<CreateHostAttachment, AppError> {
        Ok(CreateHostAttachment::new(
            host_name,
            CidrValue::new(self.network)?,
            self.mac_address.map(MacAddressValue::new).transpose()?,
            self.comment,
        ))
    }
}

#[derive(Deserialize, ToSchema)]
pub struct UpdateHostAttachmentRequest {
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    mac_address: UpdateField<String>,
    #[serde(default)]
    #[schema(value_type = Option<String>)]
    comment: UpdateField<String>,
}

impl UpdateHostAttachmentRequest {
    fn into_command(self) -> Result<UpdateHostAttachment, AppError> {
        Ok(UpdateHostAttachment {
            mac_address: self.mac_address.try_map(MacAddressValue::new)?,
            comment: self.comment,
        })
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttachmentDhcpIdentifierRequest {
    family: u8,
    kind: String,
    value: String,
    #[schema(default = 100)]
    priority: i32,
}

impl CreateAttachmentDhcpIdentifierRequest {
    fn into_command(self, attachment_id: Uuid) -> Result<CreateAttachmentDhcpIdentifier, AppError> {
        let family = match self.family {
            4 => DhcpIdentifierFamily::V4,
            6 => DhcpIdentifierFamily::V6,
            _ => {
                return Err(AppError::validation(
                    "attachment DHCP identifier family must be 4 or 6",
                ));
            }
        };
        let kind = match self.kind.as_str() {
            "client_id" => DhcpIdentifierKind::ClientId,
            "duid_llt" => DhcpIdentifierKind::DuidLlt,
            "duid_en" => DhcpIdentifierKind::DuidEn,
            "duid_ll" => DhcpIdentifierKind::DuidLl,
            "duid_uuid" => DhcpIdentifierKind::DuidUuid,
            "duid_raw" => DhcpIdentifierKind::DuidRaw,
            _ => {
                return Err(AppError::validation(
                    "unsupported attachment DHCP identifier kind",
                ));
            }
        };
        CreateAttachmentDhcpIdentifier::new(
            attachment_id,
            family,
            kind,
            self.value,
            DhcpPriority::new(self.priority),
        )
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttachmentPrefixReservationRequest {
    prefix: String,
}

impl CreateAttachmentPrefixReservationRequest {
    fn into_command(
        self,
        attachment_id: Uuid,
    ) -> Result<CreateAttachmentPrefixReservation, AppError> {
        CreateAttachmentPrefixReservation::new(attachment_id, CidrValue::new(self.prefix)?)
    }
}

#[derive(Deserialize, ToSchema)]
pub struct CreateAttachmentIpAddressRequest {
    address: Option<String>,
}

async fn ip_page_for_attachment<'a>(
    state: &'a AppState,
    attachment: &'a HostAttachment,
) -> Result<Vec<IpAddressResponse>, AppError> {
    let page = state
        .services
        .hosts()
        .list_host_ip_addresses(
            attachment.host_name(),
            &crate::domain::pagination::PageRequest::all(),
        )
        .await?;
    Ok(page
        .items
        .into_iter()
        .filter(|assignment| assignment.attachment_id() == attachment.id())
        .map(|assignment| IpAddressResponse::from_domain(&assignment))
        .collect())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
    cfg.service(list_host_attachments)
        .service(create_host_attachment)
        .service(get_attachment)
        .service(update_attachment)
        .service(delete_attachment)
        .service(assign_ip_to_attachment)
        .service(create_attachment_dhcp_identifier)
        .service(delete_attachment_dhcp_identifier)
        .service(create_attachment_prefix_reservation)
        .service(delete_attachment_prefix_reservation);
}

#[get("/inventory/hosts/{name}/attachments")]
pub(crate) async fn list_host_attachments(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
) -> Result<HttpResponse, AppError> {
    let host_name = Hostname::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_attachment::LIST,
            authz::actions::resource_kinds::HOST_ATTACHMENT,
            host_name.as_str(),
        )
        .attr(
            "host_name",
            AttrValue::String(host_name.as_str().to_string()),
        )
        .build(),
    )
    .await?;
    let items = state
        .services
        .attachments()
        .list_attachments_for_host(&host_name)
        .await?;
    Ok(HttpResponse::Ok().json(
        items
            .iter()
            .map(HostAttachmentResponse::from_domain)
            .collect::<Vec<_>>(),
    ))
}

#[post("/inventory/hosts/{name}/attachments")]
pub(crate) async fn create_host_attachment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<String>,
    payload: web::Json<CreateHostAttachmentRequest>,
) -> Result<HttpResponse, AppError> {
    let host_name = Hostname::new(path.into_inner())?;
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_attachment::CREATE,
            authz::actions::resource_kinds::HOST_ATTACHMENT,
            host_name.as_str(),
        )
        .attr(
            "host_name",
            AttrValue::String(host_name.as_str().to_string()),
        )
        .build(),
    )
    .await?;
    let attachment = state
        .services
        .attachments()
        .create_attachment(payload.into_inner().into_command(host_name)?)
        .await?;
    Ok(HttpResponse::Created().json(HostAttachmentResponse::from_domain(&attachment)))
}

#[get("/inventory/attachments/{attachment_id}")]
pub(crate) async fn get_attachment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let attachment_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_attachment::GET,
            authz::actions::resource_kinds::HOST_ATTACHMENT,
            attachment_id.to_string(),
        )
        .build(),
    )
    .await?;
    let attachment = state
        .services
        .attachments()
        .get_attachment(attachment_id)
        .await?;
    let ip_addresses = ip_page_for_attachment(state.get_ref(), &attachment).await?;
    let dhcp_identifiers = state
        .services
        .attachments()
        .list_attachment_dhcp_identifiers(attachment_id)
        .await?
        .iter()
        .map(AttachmentDhcpIdentifierResponse::from_domain)
        .collect();
    let prefix_reservations = state
        .services
        .attachments()
        .list_attachment_prefix_reservations(attachment_id)
        .await?
        .iter()
        .map(AttachmentPrefixReservationResponse::from_domain)
        .collect();
    Ok(HttpResponse::Ok().json(HostAttachmentDetailResponse {
        attachment: HostAttachmentResponse::from_domain(&attachment),
        ip_addresses,
        dhcp_identifiers,
        prefix_reservations,
    }))
}

#[patch("/inventory/attachments/{attachment_id}")]
pub(crate) async fn update_attachment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<UpdateHostAttachmentRequest>,
) -> Result<HttpResponse, AppError> {
    let attachment_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_attachment::UPDATE,
            authz::actions::resource_kinds::HOST_ATTACHMENT,
            attachment_id.to_string(),
        )
        .build(),
    )
    .await?;
    let attachment = state
        .services
        .attachments()
        .update_attachment(attachment_id, payload.into_inner().into_command()?)
        .await?;
    Ok(HttpResponse::Ok().json(HostAttachmentResponse::from_domain(&attachment)))
}

#[delete("/inventory/attachments/{attachment_id}")]
pub(crate) async fn delete_attachment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
) -> Result<HttpResponse, AppError> {
    let attachment_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::host_attachment::DELETE,
            authz::actions::resource_kinds::HOST_ATTACHMENT,
            attachment_id.to_string(),
        )
        .build(),
    )
    .await?;
    state
        .services
        .attachments()
        .delete_attachment(attachment_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/inventory/attachments/{attachment_id}/ip-addresses")]
pub(crate) async fn assign_ip_to_attachment(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<CreateAttachmentIpAddressRequest>,
) -> Result<HttpResponse, AppError> {
    let attachment = state
        .services
        .attachments()
        .get_attachment(path.into_inner())
        .await?;
    let manual = payload.address.is_some();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            if manual {
                authz::actions::host::ip::ASSIGN_MANUAL
            } else {
                authz::actions::host::ip::ASSIGN_AUTO
            },
            authz::actions::resource_kinds::IP_ADDRESS,
            attachment.id().to_string(),
        )
        .attr(
            "host_name",
            AttrValue::String(attachment.host_name().as_str().to_string()),
        )
        .attr(
            "network",
            AttrValue::String(attachment.network_cidr().as_str()),
        )
        .build(),
    )
    .await?;
    let request = payload.into_inner();
    let assignment = state
        .services
        .hosts()
        .assign_ip_address(AssignIpAddress::new(
            attachment.host_name().clone(),
            request.address.map(IpAddressValue::new).transpose()?,
            Some(attachment.network_cidr().clone()),
            attachment.mac_address().cloned(),
        )?)
        .await?;
    Ok(HttpResponse::Created().json(IpAddressResponse::from_domain(&assignment)))
}

#[post("/inventory/attachments/{attachment_id}/dhcp-identifiers")]
pub(crate) async fn create_attachment_dhcp_identifier(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<CreateAttachmentDhcpIdentifierRequest>,
) -> Result<HttpResponse, AppError> {
    let attachment_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_dhcp_identifier::CREATE,
            authz::actions::resource_kinds::ATTACHMENT_DHCP_IDENTIFIER,
            attachment_id.to_string(),
        )
        .build(),
    )
    .await?;
    let item = state
        .services
        .attachments()
        .create_attachment_dhcp_identifier(payload.into_inner().into_command(attachment_id)?)
        .await?;
    Ok(HttpResponse::Created().json(AttachmentDhcpIdentifierResponse::from_domain(&item)))
}

#[delete("/inventory/attachments/{attachment_id}/dhcp-identifiers/{identifier_id}")]
pub(crate) async fn delete_attachment_dhcp_identifier(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    let (attachment_id, identifier_id) = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_dhcp_identifier::DELETE,
            authz::actions::resource_kinds::ATTACHMENT_DHCP_IDENTIFIER,
            identifier_id.to_string(),
        )
        .build(),
    )
    .await?;
    state
        .services
        .attachments()
        .delete_attachment_dhcp_identifier(attachment_id, identifier_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}

#[post("/inventory/attachments/{attachment_id}/prefix-reservations")]
pub(crate) async fn create_attachment_prefix_reservation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<Uuid>,
    payload: web::Json<CreateAttachmentPrefixReservationRequest>,
) -> Result<HttpResponse, AppError> {
    let attachment_id = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_prefix_reservation::CREATE,
            authz::actions::resource_kinds::ATTACHMENT_PREFIX_RESERVATION,
            attachment_id.to_string(),
        )
        .build(),
    )
    .await?;
    let item = state
        .services
        .attachments()
        .create_attachment_prefix_reservation(payload.into_inner().into_command(attachment_id)?)
        .await?;
    Ok(HttpResponse::Created().json(AttachmentPrefixReservationResponse::from_domain(&item)))
}

#[delete("/inventory/attachments/{attachment_id}/prefix-reservations/{reservation_id}")]
pub(crate) async fn delete_attachment_prefix_reservation(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(Uuid, Uuid)>,
) -> Result<HttpResponse, AppError> {
    let (attachment_id, reservation_id) = path.into_inner();
    require_permission(
        &state.authz,
        authz_request(
            &req,
            authz::actions::attachment_prefix_reservation::DELETE,
            authz::actions::resource_kinds::ATTACHMENT_PREFIX_RESERVATION,
            reservation_id.to_string(),
        )
        .build(),
    )
    .await?;
    state
        .services
        .attachments()
        .delete_attachment_prefix_reservation(attachment_id, reservation_id)
        .await?;
    Ok(HttpResponse::NoContent().finish())
}
