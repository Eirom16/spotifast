//! Private Music request identity. Serialize only into the protected store or
//! the private native auth handoff, never diagnostics, catalog state or logs.
use crate::{Error, authorization};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const LIMIT: usize = 64 * 1024;
fn primary_user() -> String {
    "0".into()
}

// Deliberately no Debug: these fields are credentials, including visitor data.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Session {
    schema: u8,
    pub(crate) cookie: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) visitor_data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data_sync_id: Option<String>,
    #[serde(default = "primary_user")]
    pub(crate) auth_user: String,
}

impl Session {
    pub(crate) fn parse(input: &str) -> Result<Self, Error> {
        if input.len() > LIMIT {
            return Err(Error::TooLarge);
        }
        let mut session = if input.trim_start().starts_with('{') {
            let value: Value = serde_json::from_str(input).map_err(|_| Error::InvalidResponse)?;
            if value.get("schema").is_some() {
                serde_json::from_value::<Self>(value).map_err(|_| Error::InvalidResponse)?
            } else {
                Self::cookie_only(crate::legacy_cookie(input.as_bytes())?)
            }
        } else {
            Self::cookie_only(crate::paste_cookie(input)?)
        };
        if session.schema != 1 {
            return Err(Error::Unsupported);
        }
        if authorization(&session.cookie, 0).is_none()
            || !session.cookie.bytes().all(|c| (32..127).contains(&c))
            || session.auth_user.is_empty()
            || session.auth_user.len() > 2
            || !session.auth_user.bytes().all(|c| c.is_ascii_digit())
        {
            return Err(Error::InvalidRequest);
        }
        for field in [&mut session.visitor_data, &mut session.data_sync_id] {
            if field.as_ref().is_some_and(String::is_empty) {
                *field = None;
            }
            if field
                .as_ref()
                .is_some_and(|s| s.len() > 8192 || !s.bytes().all(|c| (33..127).contains(&c)))
            {
                return Err(Error::InvalidRequest);
            }
        }
        if let Some(id) = &mut session.data_sync_id
            && let Some((channel, _)) = id.split_once("||")
        {
            *id = channel.to_owned();
        }
        if session.data_sync_id.as_ref().is_some_and(String::is_empty) {
            session.data_sync_id = None;
        }
        Ok(session)
    }

    fn cookie_only(cookie: String) -> Self {
        Self {
            schema: 1,
            cookie,
            visitor_data: None,
            data_sync_id: None,
            auth_user: primary_user(),
        }
    }

    pub(crate) fn context(&self, context: &mut Value) {
        if let Some(visitor) = &self.visitor_data {
            context["client"]["visitorData"] = visitor.clone().into();
        }
        if let Some(channel) = &self.data_sync_id {
            context["user"]["onBehalfOfUser"] = channel.clone().into();
        }
    }

    fn encode(&self) -> Result<String, Error> {
        // Keep existing cookie-only sessions readable by earlier builds.
        let value = if self.visitor_data.is_none()
            && self.data_sync_id.is_none()
            && self.auth_user == "0"
        {
            self.cookie.clone()
        } else {
            serde_json::to_string(self).map_err(|_| Error::InvalidResponse)?
        };
        if value.len() > LIMIT {
            return Err(Error::TooLarge);
        }
        Ok(value)
    }
}

/// Canonical protected credential payload. The caller must verify the musical
/// account before saving this value. Do not print or include it in snapshots.
pub fn credential_document(input: &str) -> Result<String, Error> {
    Session::parse(input)?.encode()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn service_context_survives_protected_round_trip_and_old_cookies_remain_compatible() {
        let input = json!({"schema":1,"cookie":"SAPISID=synthetic-only","visitor_data":"fixture-visitor","data_sync_id":"fixture-channel||fixture-account","auth_user":"1"}).to_string();
        let stored = credential_document(&input).unwrap();
        let session = Session::parse(&stored).unwrap();
        assert_eq!(session.auth_user, "1");
        let mut context = json!({"client":{"clientName":"WEB_REMIX"}});
        session.context(&mut context);
        assert_eq!(context["client"]["visitorData"], "fixture-visitor");
        assert_eq!(context["user"]["onBehalfOfUser"], "fixture-channel");
        assert_eq!(
            credential_document("SAPISID=old-fixture").unwrap(),
            "SAPISID=old-fixture"
        );
    }
    #[test]
    fn invalid_or_future_context_never_becomes_a_session() {
        for input in [
            json!({"schema":2,"cookie":"SAPISID=fixture"}),
            json!({"schema":1,"cookie":"SID=anonymous","visitor_data":"fixture"}),
            json!({"schema":1,"cookie":"SAPISID=fixture","auth_user":"0\r\ninjected"}),
            json!({"schema":1,"cookie":"SAPISID=fixture","visitor_data":"bad\ncontext"}),
        ] {
            assert!(Session::parse(&input.to_string()).is_err());
        }
        assert_eq!(
            credential_document(&"x".repeat(LIMIT + 1)).err(),
            Some(Error::TooLarge)
        );
    }
}
