// Copyright 2026, Jeroen van Erp <jeroen@geeko.me>
// SPDX-License-Identifier: Apache-2.0

//! Test utilities for mocking Kubernetes API responses.

use http::{Request, Response};
use kube::client::Body;
use kube::Client;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use tower::Service;

/// A mock HTTP service that returns predefined responses based on request paths.
#[derive(Clone)]
pub struct MockService {
    responses: Arc<Mutex<HashMap<(String, String), (u16, String)>>>,
}

impl MockService {
    pub fn new() -> Self {
        Self {
            responses: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Add a response for GET requests matching the exact path
    pub fn on_get(self, path: &str, status: u16, body: &str) -> Self {
        self.responses
            .lock()
            .unwrap()
            .insert(("GET".to_string(), path.to_string()), (status, body.to_string()));
        self
    }

    /// Add a response for POST requests matching the exact path
    pub fn on_post(self, path: &str, status: u16, body: &str) -> Self {
        self.responses
            .lock()
            .unwrap()
            .insert(("POST".to_string(), path.to_string()), (status, body.to_string()));
        self
    }

    /// Build a kube Client from this mock service
    pub fn into_client(self) -> Client {
        Client::new(self, "https://kubernetes.default.svc")
    }

    fn find_response(&self, method: &str, path: &str) -> Option<(u16, String)> {
        let responses = self.responses.lock().unwrap();

        // Try exact match first
        if let Some(resp) = responses.get(&(method.to_string(), path.to_string())) {
            return Some(resp.clone());
        }

        // Try prefix match for paths like /api/v1/namespaces/foo
        for ((m, p), resp) in responses.iter() {
            if m == method && path.starts_with(p) {
                return Some(resp.clone());
            }
        }

        None
    }
}

impl Default for MockService {
    fn default() -> Self {
        Self::new()
    }
}

impl Service<Request<Body>> for MockService {
    type Response = Response<Body>;
    type Error = tower::BoxError;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Self::Response, Self::Error>> + Send>,
    >;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let method = req.method().to_string();
        let path = req.uri().path().to_string();

        let response = self.find_response(&method, &path);

        Box::pin(async move {
            match response {
                Some((status, body)) => Ok(Response::builder()
                    .status(status)
                    .header("content-type", "application/json")
                    .body(Body::from(body.into_bytes()))
                    .unwrap()),
                None => {
                    // Default 404 for unmatched requests
                    let body = r#"{"kind":"Status","apiVersion":"v1","status":"Failure","message":"not found","reason":"NotFound","code":404}"#;
                    Ok(Response::builder()
                        .status(404)
                        .header("content-type", "application/json")
                        .body(Body::from(body.as_bytes().to_vec()))
                        .unwrap())
                }
            }
        })
    }
}

/// Create a mock namespace JSON response
pub fn namespace_json(name: &str) -> String {
    serde_json::json!({
        "apiVersion": "v1",
        "kind": "Namespace",
        "metadata": {
            "name": name,
            "uid": "test-uid"
        }
    })
    .to_string()
}

/// Create a 404 not found response
pub fn not_found_json(resource: &str, name: &str) -> String {
    serde_json::json!({
        "kind": "Status",
        "apiVersion": "v1",
        "status": "Failure",
        "message": format!("{} \"{}\" not found", resource, name),
        "reason": "NotFound",
        "code": 404
    })
    .to_string()
}
