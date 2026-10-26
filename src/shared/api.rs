use reqwest::Client;
use serde::{Serialize, de::DeserializeOwned};
use std::time::Duration;
use url::Url;

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
}

impl Api {
    pub fn new(http: Client, input: &str) -> Result<Self, String> {
        Ok(Self {
            base: normalize_instance(input)?,
            http,
            token: None,
        })
    }

    pub fn authenticated(mut self, token: String) -> Self {
        self.token = Some(token);
        self
    }
    pub async fn post<Q: Serialize, R: DeserializeOwned>(
        &self,
        endpoint: &str,
        body: &Q,
    ) -> Result<R, String> {
        let url = self
            .base
            .join(&format!("api/v3/{endpoint}"))
            .map_err(|e| e.to_string())?;
        let mut request = self.http.post(url).json(body);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|_| "Could not reach instance".to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "Request failed (HTTP {}). Check credentials, two-factor code, or session.",
                response.status()
            ));
        }
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
        let url = self
            .base
            .join(&format!("api/v3/{endpoint}"))
            .map_err(|e| e.to_string())?;
        let mut request = self.http.get(url).query(query);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|e| format!("Could not reach {}: {e}", self.base))?;
        let status = response.status();
        if !status.is_success() {
            return Err(match status.as_u16() {
                401 | 403 => "This instance requires login or denied this request.".into(),
                404 => {
                    "Lemmy API v3 was not found. This reader supports Lemmy 0.19 instances.".into()
                }
                429 => "This instance is rate limiting requests. Wait a moment and retry.".into(),
                _ => format!("The instance returned HTTP {status}. Please retry."),
            });
        }
        response
            .json()
            .await
            .map_err(|e| format!("Unexpected Lemmy API v3 response: {e}"))
    }
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
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

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

    async fn serve(status: &str, body: &str) -> (Api, tokio::task::JoinHandle<String>) {
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
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0; 8192];
            let count = socket.read(&mut request).await.unwrap();
            socket.write_all(reply.as_bytes()).await.unwrap();
            String::from_utf8_lossy(&request[..count]).into_owned()
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
