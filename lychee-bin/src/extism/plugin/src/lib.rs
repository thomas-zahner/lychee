use extism_pdk::*;
use serde::{Deserialize, Serialize};

#[plugin_fn]
pub fn chain(request: ExtismRequest) -> FnResult<ExtismChainResult> {
    // request.url = format!("{}/foo", request.url);
    Ok(ExtismChainResult::Next(request))
    // Ok(ExtismChainResult::Done(ExtismStatus::Excluded))
    // Ok(ExtismChainResult::Done(ExtismStatus::Excluded))
}

// - - - - - - - - - Copy paste begin - - - - - - - - - - -
#[derive(Debug, ToBytes, Serialize, FromBytes, Deserialize)]
#[encoding(Json)]
enum ExtismChainResult {
    Next(ExtismRequest),
    Done(ExtismStatus),
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
