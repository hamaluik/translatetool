use rustls::{
    self,
    internal::pemfile,
    sign::{self, SigningKey},
    PrivateKey,
};
use serde::{Deserialize, Serialize};
use std::io;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ServiceAccountKey {
    #[serde(rename = "type")]
    key_type: String,
    project_id: String,
    private_key_id: String,
    private_key: String,
    client_email: String,
    client_id: String,
    auth_uri: String,
    token_uri: String,
    auth_provider_x509_cert_url: String,
    client_x509_cert_url: String,
}

fn decode_rsa_key(pem_pkcs8: &str) -> Result<PrivateKey, io::Error> {
    let private = pem_pkcs8.to_string().replace("\\n", "\n").into_bytes();
    let mut private_reader: &[u8] = private.as_ref();
    let private_keys = pemfile::pkcs8_private_keys(&mut private_reader);

    if let Ok(pk) = private_keys {
        if pk.len() > 0 {
            Ok(pk[0].clone())
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Not enough private keys in PEM",
            ))
        }
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Error reading key from PEM",
        ))
    }
}

struct ServiceToken {
    access_token: String,
    expires_at: u64,
}

pub struct ServiceCredentials {
    scope: String,
    credentials: ServiceAccountKey,
    token: Option<ServiceToken>,
}

#[derive(Deserialize, Serialize)]
struct AuthClaims<'a> {
    iss: &'a str,
    scope: &'static str,
    aud: &'static str,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize, Serialize)]
struct AuthResp {
    access_token: String,
    expires_in: u64,
    token_type: String,
}

/// Encodes s as Base64
fn encode_base64<T: AsRef<[u8]>>(s: T) -> String {
    base64::encode_config(s.as_ref(), base64::URL_SAFE)
}

const GOOGLE_RS256_HEAD: &'static str = "{\"alg\":\"RS256\",\"typ\":\"JWT\"}";

/// Permissions requested for a JWT.
/// See https://developers.google.com/identity/protocols/OAuth2ServiceAccount#authorizingrequests.
#[derive(Serialize, Debug)]
struct Claims {
    iss: String,
    aud: String,
    exp: u64,
    iat: u64,
    sub: Option<String>,
    scope: String,
}

/// A JSON Web Token ready for signing.
struct JWT {
    /// The value of GOOGLE_RS256_HEAD.
    header: String,
    /// A Claims struct, expressing the set of desired permissions etc.
    claims: Claims,
}

impl JWT {
    /// Create a new JWT from claims.
    fn new(claims: Claims) -> JWT {
        JWT {
            header: GOOGLE_RS256_HEAD.to_string(),
            claims: claims,
        }
    }

    /// Set JWT header. Default is `{"alg":"RS256","typ":"JWT"}`.
    #[allow(dead_code)]
    pub fn set_header(&mut self, head: String) {
        self.header = head;
    }

    /// Encodes the first two parts (header and claims) to base64 and assembles them into a form
    /// ready to be signed.
    fn encode_claims(&self) -> String {
        let mut head = encode_base64(&self.header);
        let claims = encode_base64(serde_json::to_string(&self.claims).unwrap());

        head.push_str(".");
        head.push_str(&claims);
        head
    }

    /// Sign a JWT base string with `private_key`, which is a PKCS8 string.
    fn sign(&self, private_key: &str) -> Result<String, io::Error> {
        let mut jwt_head = self.encode_claims();
        let key = decode_rsa_key(private_key)?;
        let signing_key = sign::RSASigningKey::new(&key)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Couldn't initialize signer"))?;
        let signer = signing_key
            .choose_scheme(&[rustls::SignatureScheme::RSA_PKCS1_SHA256])
            .ok_or(io::Error::new(
                io::ErrorKind::Other,
                "Couldn't choose signing scheme",
            ))?;
        let signature = signer
            .sign(jwt_head.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("{}", e)))?;
        let signature_b64 = encode_base64(signature);

        jwt_head.push_str(".");
        jwt_head.push_str(&signature_b64);

        Ok(jwt_head)
    }
}

impl ServiceCredentials {
    pub fn load<P: AsRef<std::path::Path>>(path: P, scope: &str) -> Result<ServiceCredentials, std::io::Error> {
        let file = std::fs::File::open(path)?;
        let credentials: ServiceAccountKey = serde_json::from_reader(&file)?;
        Ok(ServiceCredentials {
            credentials,
            scope: scope.to_owned(),
            token: None,
        })
    }

    pub fn get_project_id(&self) -> String {
        self.credentials.project_id.clone()
    }

    pub fn get_access_token(&mut self) -> Result<String, Box<dyn std::error::Error>> {
        let now = SystemTime::now();
        let since = now.duration_since(UNIX_EPOCH).expect("monotonic time");
        let now = since.as_secs();

        if self.token.is_none() || self.token.as_ref().unwrap().expires_at <= now {
            // need a new token

            let claims = Claims {
                iss: self.credentials.client_email.clone(),
                aud: "https://www.googleapis.com/oauth2/v4/token".to_owned(),
                exp: now + 3600,
                iat: now,
                sub: None,
                scope: self.scope.clone(),
            };
            let jwt = JWT::new(claims);
            let claims_token = jwt.sign(&self.credentials.private_key)?;

            // request an access token from Google
            let client = reqwest::Client::new();
            let mut res = client
                .post("https://www.googleapis.com/oauth2/v4/token")
                .header(
                    reqwest::header::CONTENT_TYPE,
                    "application/x-www-form-urlencoded",
                )
                .body(format!(
                    "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
                    claims_token
                ))
                .send()?;

            // make sure it's good
            if !res.status().is_success() {
                return Err(Box::from(format!(
                    "failed to get access token: code {}: {:?}",
                    res.status(),
                    res.text().expect("text body")
                )));
            }

            // parse it
            let resp: AuthResp = res.json()?;

            // and then store it!
            self.token = Some(ServiceToken {
                access_token: resp.access_token,
                expires_at: now + 3600,
            });
        }

        Ok(self.token.as_ref().unwrap().access_token.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn can_get_google_token() {
        let mut credentials = ServiceCredentials::load("fuelgauge-221218-c727995f09a3.json", "https://www.googleapis.com/auth/datastore https://www.googleapis.com/auth/firebase.messaging")
            .expect("can load service credentials from file");
        let token = credentials.get_access_token().expect("can get token");
        println!("token: {}", token);
        assert!(!token.is_empty());
    }
}
