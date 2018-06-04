use futures::future::Either;
use futures::{future, Future, Stream};
use hyper;
use hyper::header::{ContentLength, ContentType};
use hyper::server::{Request, Response, Service};
use hyper::{Method, StatusCode};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json;
use std::sync::Arc;

error_chain! {
    errors {
        NotFound(obj: String) {
            description("object not found")
            display("object {} not found", obj)
        }

        InternalError(s: String) {
            description("internal server error")
            display("internal server error {}", s)
        }

        BadRequest(s: String) {
            description("bad request")
            display("bad request {}", s)
        }

        MethodNotAllowed {
            description("method not allowed")
            display("method not allowed")
        }
    }
}

pub struct JsonServer<S> {
    pub inner: Arc<S>,
}

fn error_to_response(error: Error) -> Response {
    let (status, body) = match error.kind() {
        &ErrorKind::NotFound(_) => (StatusCode::NotFound, format!("{}", error)),
        &ErrorKind::BadRequest(_) => (StatusCode::BadRequest, format!("{}", error)),
        &ErrorKind::InternalError(_) => (StatusCode::InternalServerError, format!("{}", error)),
        _ => (StatusCode::InternalServerError, format!("{}", error)),
    };
    let resp = json!({
        "error": body,
    });
    let body = resp.to_string();
    let body_len = body.len() as u64;
    Response::new()
        .with_body(body)
        .with_header(ContentLength(body_len))
        .with_header(ContentType::json())
        .with_status(status)
}

impl<S: Service + JsonService + 'static> Service for JsonServer<S> {
    type Request = Request;
    type Response = Response;
    type Error = hyper::Error;
    type Future = Box<Future<Item = self::Response, Error = hyper::error::Error>>;

    fn call(&self, req: Request) -> Self::Future {
        let service = self.inner.clone();
        Box::new(if *req.method() == Method::Post {
            match service.deserialize(req.path(), req.method()) {
                Ok(f) => {
                    let req = req.body()
                        .concat2()
                        .map_err(move |e| ErrorKind::InternalError(e.to_string()).into())
                        .and_then(move |chunk| f(chunk.as_ref()));
                    let res = req.and_then(move |req| {
                        service
                            .call(req)
                            .then(move |res| match service.serialize(res) {
                                Ok(body) => {
                                    let len = body.len() as u64;
                                    let resp = Response::new()
                                        .with_body(body)
                                        .with_header(ContentLength(len))
                                        .with_header(ContentType::json())
                                        .with_status(StatusCode::Ok);
                                    future::ok(resp)
                                }
                                Err(e) => future::ok(error_to_response(e)),
                            })
                    }).or_else(|e| future::ok(error_to_response(e)));
                    Either::A(res)
                }
                Err(e) => Either::B(future::ok(error_to_response(e))),
            }
        } else {
            Either::B(future::ok(error_to_response(
                ErrorKind::MethodNotAllowed.into(),
            )))
        })
    }
}

pub trait JsonService
where
    Self: Service,
{
    fn deserialize(
        &self,
        path: &str,
        method: &Method,
    ) -> Result<fn(&[u8]) -> Result<<Self as Service>::Request>>;
    fn serialize(
        &self,
        resp: ::std::result::Result<Self::Response, <Self as Service>::Error>,
    ) -> Result<Vec<u8>>;
}

impl<S> JsonService for S
where
    S: Service,
    <S as Service>::Request: DeserializeOwned + 'static,
    <S as Service>::Response: Serialize,
    <S as Service>::Error: Into<Error>,
{
    fn deserialize(
        &self,
        _path: &str,
        _method: &Method,
    ) -> Result<fn(&[u8]) -> Result<<S as Service>::Request>> {
        Ok(|body| match serde_json::from_slice(body) {
            Ok(vec) => return Ok(vec),
            Err(e) => return Err(ErrorKind::BadRequest(e.to_string()).into()),
        })
    }

    fn serialize(
        &self,
        resp: ::std::result::Result<S::Response, <S as Service>::Error>,
    ) -> Result<Vec<u8>> {
        match resp {
            Ok(res) => Ok(serde_json::to_vec(&res).unwrap()),
            Err(e) => Err(e.into()),
        }
    }
}
