//! Version checks and request retry identity.
use super::{ExpectedObject, ObjectKind, ReviewScope};
use crate::error::Result;
use crate::models as m;
use connectrpc::ConnectError;
use serde::Serialize;

/// Optimistic version and durable retry identity for a single normalized request.
#[derive(Clone, Debug)]
pub struct MutationGuard {
    pub expected_version: Option<i64>,
    pub request_key: String,
    pub(crate) payload_hash: String,
    /// Internal CAS version after an observation; excluded from the consumer request fingerprint.
    prepared_version: Option<i64>,
    pub(crate) expected_object: Option<ExpectedObject>,
    pub(crate) scope: Option<ReviewScope>,
}

impl MutationGuard {
    // Public ------------------------------------------------------------------

    /// Bind retry identity to service-normalized input, including the supplied version.
    pub(crate) fn bind<T: Serialize>(&self, input: &T) -> Result<Self> {
        use sha2::{Digest, Sha256};
        Ok(Self {
            expected_version: self.expected_version,
            prepared_version: self.prepared_version,
            request_key: self.request_key.clone(),
            expected_object: self.expected_object.clone(),
            scope: self.scope.clone(),
            payload_hash: format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    self.expected_version,
                    &self.expected_object,
                    &self.scope,
                    input
                ))?)
            ),
        })
    }

    /// Bind hidden notes writers to the location scope, not to an internal session attempt.
    pub(crate) fn bind_attempt<T: Serialize>(
        &self,
        session_id: &uuid::Uuid,
        input: &T,
    ) -> Result<Self> {
        let attempt = if matches!(self.scope, Some(ReviewScope::Notes { .. })) {
            None
        } else {
            Some(session_id)
        };
        self.bind(&(attempt, input))
    }

    pub(crate) fn after_observation(mut self, version: i64) -> Self {
        self.prepared_version = Some(version);
        self
    }

    pub(crate) fn write_version(&self) -> Option<i64> {
        self.prepared_version.or(self.expected_version)
    }

    pub fn in_scope(mut self, scope: ReviewScope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub(crate) fn with_expected_object(mut self, object: ExpectedObject) -> Self {
        self.expected_object = Some(object);
        self
    }

    pub(crate) fn validate_object<T: Serialize>(
        &self,
        kind: ObjectKind,
        id: uuid::Uuid,
        value: &T,
    ) -> Result<()> {
        if let Some(expected) = &self.expected_object
            && (expected.kind != kind
                || expected.id != id
                || crate::utils::object_hash(value)? != expected.hash)
        {
            return Err(crate::error::Error::Conflict(
                "The selected comment or thread changed; read it again before editing".into(),
            ));
        }
        Ok(())
    }

    pub fn new(expected_version: Option<i64>, request_key: String) -> Self {
        Self {
            expected_version,
            prepared_version: None,
            request_key,
            payload_hash: String::new(),
            expected_object: None,
            scope: None,
        }
    }

    // RPC conversion ----------------------------------------------------------

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto_fields(
        version: &str,
        key: &str,
        require_version: bool,
    ) -> std::result::Result<m::MutationGuard, ConnectError> {
        if key.trim().is_empty() || key.len() > 200 {
            return Err(ConnectError::invalid_argument(
                "Provide a bounded nonempty request key",
            ));
        }
        let expected = if version.is_empty() {
            if require_version {
                return Err(ConnectError::invalid_argument(
                    "Read the current version before writing",
                ));
            }
            None
        } else {
            Some(
                version
                    .parse::<i64>()
                    .ok()
                    .filter(|v| *v >= 0)
                    .ok_or_else(|| ConnectError::invalid_argument("Invalid saved-data version"))?,
            )
        };
        Ok(m::MutationGuard::new(expected, key.into()))
    }

    /// Validate and convert the RPC input to the domain model.
    pub(crate) fn from_proto_object_version(
        version: &str,
        key: &str,
        kind: m::ObjectKind,
    ) -> std::result::Result<m::MutationGuard, ConnectError> {
        let parts: Vec<_> = version.split(':').collect();
        if parts.len() != 4
            || parts[0] != kind.as_str()
            || parts[3].len() != 64
            || !parts[3].bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(ConnectError::invalid_argument(
                "Use the selected comment or thread version returned by GetConversation",
            ));
        }
        Ok(
            m::MutationGuard::from_proto_fields(parts[1], key, true)?.with_expected_object(
                m::ExpectedObject {
                    kind,
                    id: uuid::Uuid::parse_str(parts[2])
                        .map_err(|_| ConnectError::invalid_argument("Expected UUID"))?,
                    hash: parts[3].into(),
                },
            ),
        )
    }
}
