use super::cache::{self, Cache};
use reqwest::{Client, Method, Response};
use serde::{Serialize, de::DeserializeOwned};
use std::{
    collections::HashMap,
    sync::{LazyLock, Mutex},
    time::Duration,
};
use url::Url;

const FRESH_FOR: Duration = Duration::from_secs(5 * 60);
const OFFLINE_FOR: Duration = Duration::from_secs(7 * 24 * 60 * 60);
static REVISIONS: LazyLock<Mutex<HashMap<String, u64>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Copy)]
pub enum Freshness {
    Cached,
    Refresh,
}

struct ReadError {
    message: String,
    offline: bool,
}

impl ReadError {
    fn response(message: String) -> Self {
        Self {
            message,
            offline: false,
        }
    }
}

pub fn http_client() -> Result<Client, reqwest::Error> {
    Client::builder()
        .user_agent(concat!("cosmic-ext-lemmy/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(30))
        .build()
}

/// Shared transport only. Each feature owns its endpoints and request types.
#[derive(Clone)]
pub struct Api {
    pub base: Url,
    http: Client,
    token: Option<String>,
    account: Option<String>,
}

impl Api {
    pub fn new(http: Client, input: &str) -> Result<Self, String> {
        Ok(Self {
            base: normalize_instance(input)?,
            http,
            token: None,
            account: None,
        })
    }

    pub fn authenticated(mut self, token: String) -> Self {
        self.token = Some(token);
        self.account = None;
        self
    }

    pub fn with_account_cache(mut self, account: &str) -> Self {
        self.account = Some(account.to_owned());
        self
    }

    pub(crate) fn reading_scope(&self) -> Option<String> {
        let account = if self.is_authenticated() {
            Some(self.account.as_deref()?)
        } else {
            None
        };
        Some(reading_scope(self.base.as_str(), account))
    }

    pub(crate) async fn invalidate_reading_cache(&self) {
        if let Some(scope) = self.reading_scope() {
            let _ = tokio::task::spawn_blocking(move || {
                if let Ok(mut revisions) = REVISIONS.lock() {
                    *revisions.entry(scope.clone()).or_default() += 1;
                    Cache::Reading.invalidate(&scope);
                }
            })
            .await;
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.token.is_some()
    }

    pub fn into_anonymous(mut self) -> Self {
        self.token = None;
        self.account = None;
        self
    }

    pub async fn post<Q: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &Q,
    ) -> Result<R, String> {
        self.write(Method::POST, endpoint, body).await
    }

    pub async fn put<Q: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &Q,
    ) -> Result<R, String> {
        self.write(Method::PUT, endpoint, body).await
    }

    async fn write<Q: Serialize, R: DeserializeOwned>(
        &self,
        method: Method,
        endpoint: &str,
        body: &Q,
    ) -> Result<R, String> {
        let url = self
            .base
            .join(&format!("api/v3/{endpoint}"))
            .map_err(|e| e.to_string())?;
        let mut request = self.http.request(method, url).json(body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Could not reach instance".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "The instance rejected the request (HTTP {}).",
                response.status()
            ));
        }
        self.invalidate_reading_cache().await;
        response
            .json()
            .await
            .map_err(|_| "Unexpected API response".into())
    }

    pub async fn get<Q: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &Q,
    ) -> Result<R, String> {
        self.get_response(endpoint, query)
            .await
            .map_err(|error| error.message)?
            .json()
            .await
            .map_err(|e| format!("Unexpected Lemmy API v3 response: {e}"))
    }

    pub(crate) fn account_cache_scope(instance: &str, account: &str) -> Option<String> {
        let base = normalize_instance(instance).ok()?;
        Some(reading_scope(base.as_str(), Some(account)))
    }

    pub(crate) async fn cached_snapshot<Q: Serialize + ?Sized, R: DeserializeOwned>(
        scope: &str,
        endpoint: &str,
        query: &Q,
    ) -> Option<R> {
        let key = reading_key(scope, endpoint, query)?;
        let bytes = cached_response(key, OFFLINE_FOR).await?;
        serde_json::from_slice(&bytes).ok()
    }

    pub async fn get_cached<Q: Serialize + ?Sized, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        query: &Q,
        freshness: Freshness,
    ) -> Result<R, String> {
        let Some(scope) = self.reading_scope() else {
            return self.get(endpoint, query).await;
        };
        let Some(key) = reading_key(&scope, endpoint, query) else {
            return self.get(endpoint, query).await;
        };
        let revision = REVISIONS
            .lock()
            .ok()
            .and_then(|revisions| revisions.get(&scope).copied())
            .unwrap_or_default();
        if matches!(freshness, Freshness::Cached)
            && let Some(cached) = cached_response(key.clone(), FRESH_FOR).await
            && let Ok(value) = serde_json::from_slice(&cached)
        {
            return Ok(value);
        }

        let response = match self.get_response(endpoint, query).await {
            Ok(response) => response,
            Err(error) => {
                if error.offline
                    && matches!(freshness, Freshness::Cached)
                    && let Some(cached) = cached_response(key, OFFLINE_FOR).await
                    && let Ok(value) = serde_json::from_slice(&cached)
                {
                    return Ok(value);
                }
                return Err(error.message);
            }
        };
        let bytes = response
            .bytes()
            .await
            .map_err(|e| format!("Unexpected Lemmy API v3 response: {e}"))?;
        let value = serde_json::from_slice(&bytes)
            .map_err(|e| format!("Unexpected Lemmy API v3 response: {e}"))?;
        let _ = tokio::task::spawn_blocking(move || {
            if let Ok(revisions) = REVISIONS.lock()
                && revisions.get(&scope).copied().unwrap_or_default() == revision
            {
                Cache::Reading.write(&key, &bytes);
            }
        })
        .await;
        Ok(value)
    }

    async fn get_response<Q: Serialize + ?Sized>(
        &self,
        endpoint: &str,
        query: &Q,
    ) -> Result<Response, ReadError> {
        let url = self
            .base
            .join(&format!("api/v3/{endpoint}"))
            .map_err(|e| ReadError::response(e.to_string()))?;
        let mut request = self.http.get(url).query(query);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request.send().await.map_err(|e| ReadError {
            offline: e.is_connect() || e.is_timeout(),
            message: format!("Could not reach {}: {e}", self.base),
        })?;
        let status = response.status();
        if !status.is_success() {
            return Err(ReadError::response(match status.as_u16() {
                401 | 403 => "This instance requires login or denied this request.".into(),
                404 => {
                    "Lemmy API v3 was not found. This reader supports Lemmy 0.19 instances.".into()
                }
                429 => "This instance is rate limiting requests. Wait a moment and retry.".into(),
                _ => format!("The instance returned HTTP {status}. Please retry."),
            }));
        }
        Ok(response)
    }
}

fn reading_scope(base: &str, account: Option<&str>) -> String {
    format!("{}-", cache::key((base, account)))
}

fn reading_key(scope: &str, endpoint: &str, query: &(impl Serialize + ?Sized)) -> Option<String> {
    let query = serde_json::to_vec(query).ok()?;
    Some(format!("{scope}{}", cache::key((endpoint, query))))
}

async fn cached_response(key: String, max_age: Duration) -> Option<Vec<u8>> {
    tokio::task::spawn_blocking(move || Cache::Reading.read(&key, max_age))
        .await
        .ok()?
}

pub fn normalize_instance(input: &str) -> Result<Url, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a Lemmy instance, such as lemmy.world.".into());
    }
    let input = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let mut url = Url::parse(&input).map_err(|_| "Enter a valid instance URL.".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Use an HTTP or HTTPS instance URL.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Instance URLs must not contain credentials.".into());
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err("Enter the instance root URL, without a community or post path.".into());
    }
    url.set_query(None);
    url.set_fragment(None);
    url.set_path("/");
    Ok(url)
}

impl std::fmt::Debug for Api {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Api")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

    #[tokio::test]
    async fn reuses_reading_cache_after_reopening_and_isolates_accounts() {
        let (api, request) = serve("200 OK", r#"{"value":"account one"}"#).await;
        let base = api.base.to_string();
        let account = api
            .authenticated("session-one".into())
            .with_account_cache("one");
        let value: serde_json::Value = account
            .get_cached("post/list", &(), Freshness::Cached)
            .await
            .unwrap();
        assert_eq!(value["value"], "account one");
        assert!(
            request
                .await
                .unwrap()
                .contains("authorization: Bearer session-one")
        );

        let reopened = Api::new(reqwest::Client::new(), &base)
            .unwrap()
            .authenticated("new-session".into())
            .with_account_cache("one");
        let scope = Api::account_cache_scope(&base, "one").unwrap();
        let snapshot: Option<serde_json::Value> =
            Api::cached_snapshot(&scope, "post/list", &()).await;
        assert_eq!(snapshot, Some(value.clone()));
        assert_eq!(
            reopened
                .get_cached::<_, serde_json::Value>("post/list", &(), Freshness::Cached)
                .await
                .unwrap(),
            value
        );
        assert!(
            reopened
                .get_cached::<_, serde_json::Value>("post/list", &(), Freshness::Refresh)
                .await
                .is_err()
        );

        let other = Api::account_cache_scope(&base, "two").unwrap();
        let anonymous = reopened.clone().into_anonymous().reading_scope().unwrap();
        for different_scope in [other, anonymous] {
            assert!(
                Api::cached_snapshot::<_, serde_json::Value>(&different_scope, "post/list", &())
                    .await
                    .is_none()
            );
        }

        reopened.invalidate_reading_cache().await;
        assert!(
            Api::cached_snapshot::<_, serde_json::Value>(&scope, "post/list", &())
                .await
                .is_none()
        );
    }

    #[test]
    fn normalizes_instance_and_rejects_non_instance_urls() {
        assert_eq!(
            normalize_instance(" lemmy.world ").unwrap().as_str(),
            "https://lemmy.world/"
        );
        assert_eq!(
            normalize_instance("http://localhost:8536/")
                .unwrap()
                .as_str(),
            "http://localhost:8536/"
        );
        for invalid in [
            "",
            "file:///etc/passwd",
            "https://user:secret@example.com",
            "https://lemmy.world/c/test",
        ] {
            assert!(normalize_instance(invalid).is_err(), "accepted {invalid}");
        }
    }

    pub(crate) async fn serve(status: &str, body: &str) -> (Api, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let api = Api::new(
            http_client().unwrap(),
            &format!("http://{}", listener.local_addr().unwrap()),
        )
        .unwrap();
        let reply = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let handle = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut headers = String::new();
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);
                headers.push_str(&line);
                if line == "\r\n" {
                    break;
                }
                if let Some(length) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = length.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; content_length];
            socket.read_exact(&mut body).await.unwrap();
            socket.get_mut().write_all(reply.as_bytes()).await.unwrap();
            headers + std::str::from_utf8(&body).unwrap()
        });
        (api, handle)
    }

    #[tokio::test]
    async fn authentication_is_scoped_to_each_client() {
        let (api, request) = serve("200 OK", "{}").await;
        let authenticated = api.clone().authenticated("test-token".into());
        let _: serde_json::Value = authenticated.get("site", &()).await.unwrap();
        assert!(
            request
                .await
                .unwrap()
                .to_lowercase()
                .contains("authorization: bearer test-token")
        );
        assert!(api.token.is_none());
        assert!(!format!("{authenticated:?}").contains("test-token"));
        let (anonymous, request) = serve("200 OK", "{}").await;
        let _: serde_json::Value = anonymous.get("site", &()).await.unwrap();
        assert!(
            !request
                .await
                .unwrap()
                .to_lowercase()
                .contains("authorization:")
        );
    }

    #[tokio::test]
    async fn encodes_query_and_decodes_response() {
        let (api, request) = serve("200 OK", r#"{"posts":[],"next_page":null}"#).await;
        let response: lemmy_api_common::post::GetPostsResponse = api
            .get("post/list", &[("community_name", "rust@example.com")])
            .await
            .unwrap();
        assert!(response.posts.is_empty());
        let request = request.await.unwrap();
        assert!(
            request.starts_with("GET /api/v3/post/list?community_name=rust%40example.com HTTP/1.1")
        );
        assert!(request.to_lowercase().contains("cosmic-ext-lemmy/"));
    }

    #[tokio::test]
    async fn reports_rate_limits_and_invalid_json() {
        for (status, body, expected) in [
            ("429 Too Many Requests", "{}", "rate limiting"),
            ("200 OK", "<html>oops</html>", "Unexpected Lemmy"),
        ] {
            let (api, request) = serve(status, body).await;
            let error = api
                .get::<_, serde_json::Value>("site", &())
                .await
                .unwrap_err();
            assert!(error.contains(expected), "{error}");
            request.await.unwrap();
        }
    }
}
