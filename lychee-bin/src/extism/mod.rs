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

pub(crate) fn load_extism_request_chain(paths: &[PathBuf]) -> RequestChain {
    let plugins = paths
        .iter()
        .map(|path| {
            let file = Wasm::file(path);
            let manifest = Manifest::new([file]);
            let mut plugin = Plugin::new(manifest, [], true).unwrap(); // TODO: don't unwrap

            let boxed_item: Box<dyn Chainable<reqwest::Request, Status> + std::marker::Send> =
                Box::new(ExtismChainItem {
                    function: Box::new(move |req| {
                        let r = plugin.call::<ExtismRequest, ExtismChainResult>("chain", req);
                        dbg!(&r);
                        r.unwrap() // TODO: don't unwrap
                    }),
                });

            boxed_item
        })
        .collect();

    Chain::new(plugins)
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

impl From<ExtismChainResult> for ChainResult<Request, Status> {
    fn from(val: ExtismChainResult) -> Self {
        use ExtismChainResult::{Done, Next};
        match val {
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
        use ExtismStatus::{Excluded, Ok};
        match value {
            Ok(s) => Self::Ok(s.try_into().unwrap()), // TODO
            Excluded => Self::Excluded,
        }
    }
}
