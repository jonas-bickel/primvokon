//! `/api/redfish/v1/*`

use reqwest::Method;
use serde_json::{json, Value};

use crate::client::{Body, PikvmClient, Query};
use crate::error::Result;
use crate::models::RedfishResetType;

pub struct RedfishApi<'a>(pub(crate) &'a PikvmClient);

impl RedfishApi<'_> {
    /// `GET /api/redfish/v1` – service root (no auth required by kvmd).
    pub async fn root(&self) -> Result<Value> {
        let r = self
            .0
            .call_ok(Method::GET, "/api/redfish/v1", &Query::new(), Body::Empty)
            .await?;
        r.json()
    }

    /// `GET /api/redfish/v1/Systems`
    pub async fn systems(&self) -> Result<Value> {
        let r = self
            .0
            .call_ok(Method::GET, "/api/redfish/v1/Systems", &Query::new(), Body::Empty)
            .await?;
        r.json()
    }

    /// `GET /api/redfish/v1/Systems/{id}` – `id` is `0` or `SwitchPortN`.
    pub async fn system(&self, id: &str) -> Result<Value> {
        let path = format!("/api/redfish/v1/Systems/{id}");
        let r = self.0.call_ok(Method::GET, &path, &Query::new(), Body::Empty).await?;
        r.json()
    }

    /// `PATCH /api/redfish/v1/Systems/{id}` – no-op that returns 204.
    pub async fn patch_system(&self, id: &str) -> Result<()> {
        let path = format!("/api/redfish/v1/Systems/{id}");
        self.0
            .call_ok(Method::PATCH, &path, &Query::new(), Body::Json(json!({})))
            .await?;
        Ok(())
    }

    /// `POST /api/redfish/v1/Systems/{id}/Actions/ComputerSystem.Reset`
    pub async fn reset(&self, id: &str, reset_type: RedfishResetType) -> Result<()> {
        let path = format!("/api/redfish/v1/Systems/{id}/Actions/ComputerSystem.Reset");
        let body = Body::Json(json!({ "ResetType": reset_type.as_str() }));
        self.0.call_ok(Method::POST, &path, &Query::new(), body).await?;
        Ok(())
    }
}
