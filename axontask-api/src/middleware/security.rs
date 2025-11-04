/// Security headers middleware
///
/// This module provides middleware to add security-related HTTP headers
/// to all responses, following OWASP recommendations.
///
/// # Headers Applied
///
/// - `X-Content-Type-Options: nosniff` - Prevents MIME type sniffing
/// - `X-Frame-Options: DENY` - Prevents clickjacking
/// - `X-XSS-Protection: 1; mode=block` - Enables XSS protection in older browsers
/// - `Strict-Transport-Security` - Forces HTTPS (production only)
/// - `Content-Security-Policy` - Restricts resource loading
/// - `Referrer-Policy: strict-origin-when-cross-origin` - Controls referrer information
/// - `Permissions-Policy` - Controls browser features
///
/// # Example
///
/// ```no_run
/// use axum::Router;
/// use axontask_api::middleware::security::SecurityHeadersLayer;
///
/// let app = Router::new()
///     .layer(SecurityHeadersLayer::new(true)); // true = production mode
/// ```

use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use tower::{Layer, Service};
use std::task::{Context, Poll};

/// Security headers middleware layer
#[derive(Clone)]
pub struct SecurityHeadersLayer {
    /// Whether to enable HSTS (HTTPS-only, should be true in production)
    enable_hsts: bool,
}

impl SecurityHeadersLayer {
    /// Creates a new security headers layer
    ///
    /// # Arguments
    ///
    /// * `enable_hsts` - Whether to enable HSTS header (use true for production with HTTPS)
    pub fn new(enable_hsts: bool) -> Self {
        Self { enable_hsts }
    }
}

impl<S> Layer<S> for SecurityHeadersLayer {
    type Service = SecurityHeadersMiddleware<S>;

    fn layer(&self, inner: S) -> Self::Service {
        SecurityHeadersMiddleware {
            inner,
            enable_hsts: self.enable_hsts,
        }
    }
}

/// Security headers middleware service
#[derive(Clone)]
pub struct SecurityHeadersMiddleware<S> {
    inner: S,
    enable_hsts: bool,
}

impl<S> Service<Request> for SecurityHeadersMiddleware<S>
where
    S: Service<Request, Response = Response> + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request) -> Self::Future {
        let future = self.inner.call(request);
        let enable_hsts = self.enable_hsts;

        Box::pin(async move {
            let mut response = future.await?;

            let headers = response.headers_mut();

            // Prevent MIME type sniffing
            headers.insert(
                "X-Content-Type-Options",
                "nosniff".parse().unwrap(),
            );

            // Prevent clickjacking
            headers.insert(
                "X-Frame-Options",
                "DENY".parse().unwrap(),
            );

            // Enable XSS protection (for older browsers)
            headers.insert(
                "X-XSS-Protection",
                "1; mode=block".parse().unwrap(),
            );

            // Control referrer information
            headers.insert(
                "Referrer-Policy",
                "strict-origin-when-cross-origin".parse().unwrap(),
            );

            // Disable potentially dangerous browser features
            headers.insert(
                "Permissions-Policy",
                "geolocation=(), microphone=(), camera=(), payment=(), usb=()".parse().unwrap(),
            );

            // Content Security Policy (strict)
            headers.insert(
                "Content-Security-Policy",
                "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self'; frame-ancestors 'none'".parse().unwrap(),
            );

            // HSTS (only in production with HTTPS)
            if enable_hsts {
                headers.insert(
                    "Strict-Transport-Security",
                    "max-age=31536000; includeSubDomains; preload".parse().unwrap(),
                );
            }

            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::StatusCode,
        response::IntoResponse,
        routing::get,
        Router,
    };
    use tower::Service as _;

    #[tokio::test]
    async fn test_security_headers_applied() {
        async fn handler() -> impl IntoResponse {
            (StatusCode::OK, "test")
        }

        let mut app = Router::new()
            .route("/test", get(handler))
            .layer(SecurityHeadersLayer::new(false));

        let response = app
            .call(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();

        let headers = response.headers();

        assert_eq!(
            headers.get("X-Content-Type-Options").unwrap(),
            "nosniff"
        );
        assert_eq!(
            headers.get("X-Frame-Options").unwrap(),
            "DENY"
        );
        assert_eq!(
            headers.get("X-XSS-Protection").unwrap(),
            "1; mode=block"
        );
        assert_eq!(
            headers.get("Referrer-Policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert!(headers.get("Content-Security-Policy").is_some());
        assert!(headers.get("Permissions-Policy").is_some());
    }

    #[tokio::test]
    async fn test_hsts_enabled_in_production() {
        async fn handler() -> impl IntoResponse {
            (StatusCode::OK, "test")
        }

        let mut app = Router::new()
            .route("/test", get(handler))
            .layer(SecurityHeadersLayer::new(true));

        let response = app
            .call(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();

        let headers = response.headers();
        assert!(headers.get("Strict-Transport-Security").is_some());
    }

    #[tokio::test]
    async fn test_hsts_disabled_in_dev() {
        async fn handler() -> impl IntoResponse {
            (StatusCode::OK, "test")
        }

        let mut app = Router::new()
            .route("/test", get(handler))
            .layer(SecurityHeadersLayer::new(false));

        let response = app
            .call(
                Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap()
            )
            .await
            .unwrap();

        let headers = response.headers();
        assert!(headers.get("Strict-Transport-Security").is_none());
    }
}
