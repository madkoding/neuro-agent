//! HTTP Client tool - Make HTTP requests

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// HTTP method
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

impl Default for HttpMethod {
    fn default() -> Self {
        Self::Get
    }
}

/// HTTP request arguments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpRequestArgs {
    pub url: String,
    #[serde(default)]
    pub method: HttpMethod,
    pub headers: Option<HashMap<String, String>>,
    pub body: Option<String>,
    pub json: Option<serde_json::Value>,
    pub timeout_secs: Option<u64>,
    pub follow_redirects: Option<bool>,
}

/// HTTP response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    pub status: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
    pub body_json: Option<serde_json::Value>,
    pub elapsed_ms: u64,
    pub url: String,
    pub redirected: bool,
}

/// HTTP client tool
#[derive(Debug, Clone)]
pub struct HttpClientTool {
    user_agent: String,
}

impl Default for HttpClientTool {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClientTool {
    pub const NAME: &'static str = "http_request";

    pub fn new() -> Self {
        Self {
            user_agent: format!("neuro-agent/{}", env!("CARGO_PKG_VERSION")),
        }
    }

    /// Make an HTTP request
    pub async fn request(&self, args: HttpRequestArgs) -> Result<HttpResponse, HttpError> {
        let client = reqwest::Client::builder()
            .user_agent(&self.user_agent)
            .timeout(Duration::from_secs(args.timeout_secs.unwrap_or(30)))
            .redirect(if args.follow_redirects.unwrap_or(true) {
                reqwest::redirect::Policy::limited(10)
            } else {
                reqwest::redirect::Policy::none()
            })
            .build()
            .map_err(|e| HttpError::ClientError(e.to_string()))?;

        let mut request = match args.method {
            HttpMethod::Get => client.get(&args.url),
            HttpMethod::Post => client.post(&args.url),
            HttpMethod::Put => client.put(&args.url),
            HttpMethod::Patch => client.patch(&args.url),
            HttpMethod::Delete => client.delete(&args.url),
            HttpMethod::Head => client.head(&args.url),
            HttpMethod::Options => client.request(reqwest::Method::OPTIONS, &args.url),
        };

        // Add headers
        if let Some(headers) = args.headers {
            for (key, value) in headers {
                request = request.header(&key, &value);
            }
        }

        // Add body
        if let Some(json) = args.json {
            request = request.json(&json);
        } else if let Some(body) = args.body {
            request = request.body(body);
        }

        let start = std::time::Instant::now();
        let response = request.send().await
            .map_err(|e| HttpError::RequestError(e.to_string()))?;
        let elapsed_ms = start.elapsed().as_millis() as u64;

        let status = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("Unknown").to_string();
        let final_url = response.url().to_string();
        let redirected = final_url != args.url;

        // Collect headers
        let mut headers = HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(key.to_string(), v.to_string());
            }
        }

        // Get body
        let body = response.text().await
            .map_err(|e| HttpError::ResponseError(e.to_string()))?;

        // Try to parse as JSON
        let body_json = serde_json::from_str(&body).ok();

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body,
            body_json,
            elapsed_ms,
            url: final_url,
            redirected,
        })
    }

    /// Make a GET request (convenience method)
    pub async fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        self.request(HttpRequestArgs {
            url: url.to_string(),
            method: HttpMethod::Get,
            headers: None,
            body: None,
            json: None,
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }

    /// Make a POST request with JSON (convenience method)
    pub async fn post_json(&self, url: &str, json: serde_json::Value) -> Result<HttpResponse, HttpError> {
        self.request(HttpRequestArgs {
            url: url.to_string(),
            method: HttpMethod::Post,
            headers: None,
            body: None,
            json: Some(json),
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }

    /// Download a file
    pub async fn download(&self, url: &str, path: &str) -> Result<DownloadResult, HttpError> {
        let client = reqwest::Client::builder()
            .user_agent(&self.user_agent)
            .build()
            .map_err(|e| HttpError::ClientError(e.to_string()))?;

        let start = std::time::Instant::now();
        let response = client.get(url).send().await
            .map_err(|e| HttpError::RequestError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(HttpError::RequestError(format!(
                "Download failed with status: {}", response.status()
            )));
        }

        let content_length = response.content_length();
        let bytes = response.bytes().await
            .map_err(|e| HttpError::ResponseError(e.to_string()))?;

        tokio::fs::write(path, &bytes).await
            .map_err(|e| HttpError::IoError(e.to_string()))?;

        Ok(DownloadResult {
            path: path.to_string(),
            size_bytes: bytes.len() as u64,
            elapsed_ms: start.elapsed().as_millis() as u64,
            content_length,
        })
    }

    /// Fetch and parse JSON
    pub async fn fetch_json(&self, url: &str) -> Result<serde_json::Value, HttpError> {
        let response = self.get(url).await?;
        
        response.body_json.ok_or_else(|| {
            HttpError::ParseError("Response is not valid JSON".to_string())
        })
    }
}

/// Download result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadResult {
    pub path: String,
    pub size_bytes: u64,
    pub elapsed_ms: u64,
    pub content_length: Option<u64>,
}

/// HTTP client errors
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("Client error: {0}")]
    ClientError(String),
    #[error("Request error: {0}")]
    RequestError(String),
    #[error("Response error: {0}")]
    ResponseError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("IO error: {0}")]
    IoError(String),
}

/// API client for common patterns
#[derive(Debug, Clone)]
pub struct ApiClient {
    http: HttpClientTool,
    base_url: String,
    default_headers: HashMap<String, String>,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            http: HttpClientTool::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
            default_headers: HashMap::new(),
        }
    }

    pub fn with_bearer_token(mut self, token: &str) -> Self {
        self.default_headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", token),
        );
        self
    }

    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.default_headers.insert(key.to_string(), value.to_string());
        self
    }

    pub async fn get(&self, endpoint: &str) -> Result<HttpResponse, HttpError> {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        self.http.request(HttpRequestArgs {
            url,
            method: HttpMethod::Get,
            headers: Some(self.default_headers.clone()),
            body: None,
            json: None,
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }

    pub async fn post(&self, endpoint: &str, json: serde_json::Value) -> Result<HttpResponse, HttpError> {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        self.http.request(HttpRequestArgs {
            url,
            method: HttpMethod::Post,
            headers: Some(self.default_headers.clone()),
            body: None,
            json: Some(json),
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }

    pub async fn put(&self, endpoint: &str, json: serde_json::Value) -> Result<HttpResponse, HttpError> {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        self.http.request(HttpRequestArgs {
            url,
            method: HttpMethod::Put,
            headers: Some(self.default_headers.clone()),
            body: None,
            json: Some(json),
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }

    pub async fn delete(&self, endpoint: &str) -> Result<HttpResponse, HttpError> {
        let url = format!("{}/{}", self.base_url, endpoint.trim_start_matches('/'));
        self.http.request(HttpRequestArgs {
            url,
            method: HttpMethod::Delete,
            headers: Some(self.default_headers.clone()),
            body: None,
            json: None,
            timeout_secs: None,
            follow_redirects: None,
        }).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_method_default() {
        assert_eq!(HttpMethod::default(), HttpMethod::Get);
    }

    #[test]
    fn test_api_client_url_building() {
        let client = ApiClient::new("https://api.example.com/");
        assert_eq!(client.base_url, "https://api.example.com");
    }
}
