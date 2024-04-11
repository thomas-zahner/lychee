use async_trait::async_trait;
use extism::{convert::Json, Manifest, Plugin, ToBytes, Wasm};
use extism_convert::FromBytes;
use http::Method;
use lychee_lib::{
    chain::{Chain, ChainResult, Chainable, RequestChain},
    Status,
};
use reqwest::{Request, Url};
use serde::{Deserialize, Serialize};
use std::{fmt, path::PathBuf, str::FromStr};

pub(crate) fn load_extism_request_chain(paths: &Vec<PathBuf>) -> RequestChain {
    let path = &paths[0]; // TODO
    assert!(path.is_file(), "Unable to open file"); // TODO: good error handling

    let file = Wasm::file(path);
    let manifest = Manifest::new([file]);
    let mut plugin = Plugin::new(manifest, [], true).unwrap();

    let x = Box::new(ExtismChainItem {
        function: Box::new(move |req| {
            plugin
                .call::<ExtismRequest, ExtismChainResult>("chain", req)
                .unwrap()
        }),
    });

    Chain::new(vec![x])
}

struct ExtismChainItem {
    function: Box<dyn FnMut(ExtismRequest) -> ExtismChainResult + Send>,
}

impl fmt::Debug for ExtismChainItem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExtismChainItem").finish()
    }
}

#[async_trait]
impl Chainable<Request, Status> for ExtismChainItem {
    async fn chain(&mut self, input: Request) -> ChainResult<Request, Status> {
        (self.function)(input.into()).into()
    }
}

#[derive(Debug, ToBytes, Serialize, FromBytes, Deserialize)]
#[encoding(Json)]
enum ExtismChainResult {
    Next(ExtismRequest),
    Done(ExtismStatus),
}

impl Into<ChainResult<Request, Status>> for ExtismChainResult {
    fn into(self) -> ChainResult<Request, Status> {
        use ExtismChainResult::*;
        match self {
            Next(r) => ChainResult::Next(r.into()),
            Done(s) => ChainResult::Done(s.into()),
        }
    }
}

#[derive(Debug, ToBytes, Serialize, FromBytes, Deserialize)]
#[encoding(Json)]
pub(crate) struct ExtismRequest {
    method: String,
    url: String,
}

#[derive(Debug, ToBytes, Serialize, FromBytes, Deserialize)]
#[encoding(Json)]
pub(crate) enum ExtismStatus {
    Ok(u16),
    Excluded,
}

impl From<Request> for ExtismRequest {
    fn from(value: Request) -> Self {
        Self {
            method: value.method().as_str().into(),
            url: value.url().to_string(),
        }
    }
}

impl From<ExtismRequest> for Request {
    fn from(value: ExtismRequest) -> Self {
        Request::new(
            Method::from_str(&value.method).unwrap(), // TODO
            Url::from_str(&value.url).unwrap(),       // TODO
        )
    }
}

impl From<ExtismStatus> for Status {
    fn from(value: ExtismStatus) -> Self {
        use ExtismStatus::*;
        match value {
            Ok(s) => Self::Ok(s.try_into().unwrap()),
            Excluded => Self::Excluded,
        }
    }
}
