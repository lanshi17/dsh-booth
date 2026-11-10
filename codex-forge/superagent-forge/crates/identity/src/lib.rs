//! Signing and OIDC-profile JWT identity helpers (pure Rust crypto only).
//!
//! - Merge grants are Ed25519-signed over a canonical JSON payload.
//! - Bearer tokens may be validated as HS256 JWTs with issuer/audience/exp/sub checks
//!   (OIDC access-token profile subset; no network JWKS fetch in this crate).
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use ed25519_dalek::Signature;
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey;
use ed25519_dalek::Verifier;
use ed25519_dalek::VerifyingKey;
use forge_domain::PrincipalId;
use hmac::Hmac;
use hmac::Mac;
use rand_core::OsRng;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use sha2::Sha256;
use std::collections::BTreeMap;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use thiserror::Error;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IdentityError {
    #[error("invalid signature")]
    InvalidSignature,
    #[error("invalid token: {0}")]
    InvalidToken(String),
    #[error("token expired")]
    Expired,
    #[error("issuer or audience mismatch")]
    Audience,
    #[error("subject is not authorized")]
    UnauthorizedSubject,
}

/// Canonical payload fields signed for a merge grant.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergeAuthClaims {
    pub id: Uuid,
    pub task_id: Uuid,
    pub assignment_id: Uuid,
    pub head_commit: String,
    pub base_commit: String,
    pub target_branch: String,
    pub source_branch: String,
    pub authorized_by: String,
    pub expires_at_unix: u64,
}

impl MergeAuthClaims {
    pub fn signing_bytes(&self) -> Result<Vec<u8>, IdentityError> {
        serde_json::to_vec(self).map_err(|err| IdentityError::InvalidToken(err.to_string()))
    }
}

#[derive(Clone)]
pub struct MergeAuthKeyPair {
    signing: SigningKey,
    verifying: VerifyingKey,
}

impl MergeAuthKeyPair {
    pub fn generate() -> Self {
        let signing = SigningKey::generate(&mut OsRng);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    pub fn from_signing_bytes(bytes: &[u8; 32]) -> Self {
        let signing = SigningKey::from_bytes(bytes);
        let verifying = signing.verifying_key();
        Self { signing, verifying }
    }

    pub fn verifying_key_bytes(&self) -> [u8; 32] {
        self.verifying.to_bytes()
    }

    pub fn sign(&self, claims: &MergeAuthClaims) -> Result<String, IdentityError> {
        let bytes = claims.signing_bytes()?;
        let sig = self.signing.sign(&bytes);
        Ok(URL_SAFE_NO_PAD.encode(sig.to_bytes()))
    }

    pub fn verify(&self, claims: &MergeAuthClaims, signature: &str) -> Result<(), IdentityError> {
        verify_merge_signature(&self.verifying, claims, signature)
    }
}

pub fn verify_merge_signature(
    verifying: &VerifyingKey,
    claims: &MergeAuthClaims,
    signature: &str,
) -> Result<(), IdentityError> {
    let bytes = claims.signing_bytes()?;
    let sig_bytes = URL_SAFE_NO_PAD
        .decode(signature.as_bytes())
        .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
    let sig = Signature::from_slice(&sig_bytes).map_err(|_| IdentityError::InvalidSignature)?;
    verifying
        .verify(&bytes, &sig)
        .map_err(|_| IdentityError::InvalidSignature)
}

#[derive(Debug, Clone)]
pub struct TrustedIdentity {
    pub principal: PrincipalId,
    pub can_create_tasks: bool,
    pub can_cancel: bool,
}

pub trait IdentityResolver: Send + Sync {
    fn resolve(&self, bearer: &str) -> Result<TrustedIdentity, IdentityError>;
}

#[derive(Debug, Default, Clone)]
pub struct StaticTokenMap {
    by_token: BTreeMap<String, TrustedIdentity>,
}

impl StaticTokenMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, token: impl Into<String>, identity: TrustedIdentity) {
        self.by_token.insert(token.into(), identity);
    }
}

impl IdentityResolver for StaticTokenMap {
    fn resolve(&self, bearer: &str) -> Result<TrustedIdentity, IdentityError> {
        self.by_token
            .get(bearer)
            .cloned()
            .ok_or(IdentityError::UnauthorizedSubject)
    }
}

/// OIDC-profile HS256 JWT validator (iss/aud/exp/sub). No network calls.
#[derive(Clone)]
pub struct OidcHs256Validator {
    secret: Vec<u8>,
    issuer: String,
    audience: String,
    /// Optional allowlist; empty means any subject is accepted as PrincipalId(sub).
    allowed_subjects: BTreeMap<String, TrustedIdentity>,
}

impl OidcHs256Validator {
    pub fn new(
        secret: impl Into<Vec<u8>>,
        issuer: impl Into<String>,
        audience: impl Into<String>,
    ) -> Self {
        Self {
            secret: secret.into(),
            issuer: issuer.into(),
            audience: audience.into(),
            allowed_subjects: BTreeMap::new(),
        }
    }

    pub fn allow_subject(&mut self, sub: impl Into<String>, identity: TrustedIdentity) {
        self.allowed_subjects.insert(sub.into(), identity);
    }

    pub fn issue_for_tests(&self, sub: &str, ttl_seconds: u64) -> Result<String, IdentityError> {
        let now = now_unix();
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&json!({
                "iss": self.issuer,
                "aud": self.audience,
                "sub": sub,
                "iat": now,
                "exp": now.saturating_add(ttl_seconds),
            }))
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?,
        );
        let signing_input = format!("{header}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
        mac.update(signing_input.as_bytes());
        let sig = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());
        Ok(format!("{signing_input}.{sig}"))
    }
}

impl IdentityResolver for OidcHs256Validator {
    fn resolve(&self, bearer: &str) -> Result<TrustedIdentity, IdentityError> {
        let mut parts = bearer.split('.');
        let header = parts
            .next()
            .ok_or_else(|| IdentityError::InvalidToken("missing header".into()))?;
        let payload = parts
            .next()
            .ok_or_else(|| IdentityError::InvalidToken("missing payload".into()))?;
        let signature = parts
            .next()
            .ok_or_else(|| IdentityError::InvalidToken("missing signature".into()))?;
        if parts.next().is_some() {
            return Err(IdentityError::InvalidToken("extra jwt segments".into()));
        }
        let signing_input = format!("{header}.{payload}");
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
        mac.update(signing_input.as_bytes());
        let expected = mac.finalize().into_bytes();
        let actual = URL_SAFE_NO_PAD
            .decode(signature.as_bytes())
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
        if expected.as_slice() != actual.as_slice() {
            return Err(IdentityError::InvalidSignature);
        }
        let payload_json = URL_SAFE_NO_PAD
            .decode(payload.as_bytes())
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
        let claims: JwtClaims = serde_json::from_slice(&payload_json)
            .map_err(|err| IdentityError::InvalidToken(err.to_string()))?;
        if claims.iss != self.issuer || claims.aud != self.audience {
            return Err(IdentityError::Audience);
        }
        if claims.exp <= now_unix() {
            return Err(IdentityError::Expired);
        }
        if let Some(identity) = self.allowed_subjects.get(&claims.sub) {
            return Ok(identity.clone());
        }
        if self.allowed_subjects.is_empty() {
            return Ok(TrustedIdentity {
                principal: PrincipalId(claims.sub),
                can_create_tasks: true,
                can_cancel: true,
            });
        }
        Err(IdentityError::UnauthorizedSubject)
    }
}

#[derive(Debug, Deserialize)]
struct JwtClaims {
    iss: String,
    aud: String,
    sub: String,
    exp: u64,
}

/// Try static tokens first, then OIDC JWT.
pub struct CompositeIdentityResolver {
    pub static_tokens: StaticTokenMap,
    pub oidc: Option<OidcHs256Validator>,
}

impl IdentityResolver for CompositeIdentityResolver {
    fn resolve(&self, bearer: &str) -> Result<TrustedIdentity, IdentityError> {
        if let Ok(identity) = self.static_tokens.resolve(bearer) {
            return Ok(identity);
        }
        if let Some(oidc) = &self.oidc {
            return oidc.resolve(bearer);
        }
        Err(IdentityError::UnauthorizedSubject)
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signs_and_verifies_merge_claims() {
        let keys = MergeAuthKeyPair::generate();
        let claims = MergeAuthClaims {
            id: Uuid::new_v4(),
            task_id: Uuid::new_v4(),
            assignment_id: Uuid::new_v4(),
            head_commit: "b".repeat(40),
            base_commit: "a".repeat(40),
            target_branch: "master".into(),
            source_branch: "forge/x".into(),
            authorized_by: "owner".into(),
            expires_at_unix: now_unix() + 60,
        };
        let sig = keys.sign(&claims).unwrap();
        keys.verify(&claims, &sig).unwrap();
        assert!(keys.verify(&claims, "deadbeef").is_err());
    }

    #[test]
    fn oidc_hs256_jwt_roundtrip_and_rejects_bad_aud() {
        let mut validator = OidcHs256Validator::new(b"secret", "https://issuer", "forge-api");
        validator.allow_subject(
            "user-1",
            TrustedIdentity {
                principal: PrincipalId("owner".into()),
                can_create_tasks: true,
                can_cancel: false,
            },
        );
        let token = validator.issue_for_tests("user-1", 60).unwrap();
        let identity = validator.resolve(&token).unwrap();
        assert_eq!(identity.principal.0, "owner");
        let mut other = OidcHs256Validator::new(b"secret", "https://issuer", "other-aud");
        other.allow_subject(
            "user-1",
            TrustedIdentity {
                principal: PrincipalId("owner".into()),
                can_create_tasks: true,
                can_cancel: false,
            },
        );
        assert_eq!(other.resolve(&token).unwrap_err(), IdentityError::Audience);
    }
}
