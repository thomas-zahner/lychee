use async_trait::async_trait;
use lychee_lib::{chain::RequestChain, ChainResult, ClientBuilder, Handler, Result, Status};
use reqwest::{Request, StatusCode, Url};
use serde::{Deserialize, Serialize};

const FLARE_SOLVERR_URL: &str = "http://localhost:8191/v1"; // docker run -p 8191:8191 --rm ghcr.io/flaresolverr/flaresolverr:latest
const PROTECTED_WEBSITE_TO_CHECK: &str = "https://wikipedia.org"; // just an example

#[derive(Debug)]
struct FlaresolverrProxyHandler {}

#[async_trait]
impl Handler<Request, Status> for FlaresolverrProxyHandler {
    async fn handle(&mut self, request: Request) -> ChainResult<Request, Status> {
        let client = reqwest::Client::new();

        let solver_response = client
            .post(FLARE_SOLVERR_URL)
            .json(&FlaresolverrRequest {
                url: request.url().clone(),
                cmd: Cmd::Get,
            })
            .send()
            .await
            .unwrap();

        let response = solver_response
            .json::<FlaresolverrResponse>()
            .await
            .unwrap();

        // TODO: currently always Status::Ok even when erroneous status code. Related issue: https://github.com/lycheeverse/lychee/issues/1472
        ChainResult::Done(Status::Ok(
            StatusCode::from_u16(response.solution.status).unwrap(),
        ))
    }
}

#[derive(Serialize)]
struct FlaresolverrRequest {
    url: Url,
    cmd: Cmd,
}

#[derive(Deserialize, Debug)]
struct FlaresolverrResponse {
    solution: FlaresolverrSolution,
}

#[derive(Deserialize, Debug)]
struct FlaresolverrSolution {
    status: u16,
}

#[derive(Serialize)]
enum Cmd {
    #[serde(rename(serialize = "request.get"))]
    Get,
}

#[tokio::main]
async fn main() -> Result<()> {
    let chain = RequestChain::new(vec![Box::new(FlaresolverrProxyHandler {})]);

    let client = ClientBuilder::builder()
        .plugin_request_chain(chain)
        .build()
        .client()?;

    let result = client.check(PROTECTED_WEBSITE_TO_CHECK).await;
    println!("{:?}", result);

    Ok(())
}
