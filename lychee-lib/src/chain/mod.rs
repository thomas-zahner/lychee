//! Create a `Chain` by using a `Vec` of `Box`ed `Chainable` items.
//! A `Chain` can be useful to create a modular architecture because individual items are
//! independant and easily replacable.

use crate::Status;
use async_trait::async_trait;
use core::fmt::Debug;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, PartialEq)]
/// Result of elements in the `Chain`
pub enum ChainResult<T, R> {
    /// Pass `T` to the next element in the `Chain`
    Next(T),
    /// Finish the `Chain` so that any potential subsequent `Chain` items are skipped
    Done(R),
}

/// TODO
pub type RequestChain = Chain<reqwest::Request, Status>;

pub(crate) type InnerChain<T, R> = Vec<Box<dyn Chainable<T, R> + Send>>;

/// TODO
#[derive(Debug)]
pub struct Chain<T, R>(Arc<Mutex<InnerChain<T, R>>>);

impl<T, R> Default for Chain<T, R> {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(vec![])))
    }
}

impl<T, R> Clone for Chain<T, R> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T, R> Chain<T, R> {
    /// Create a `Chain` by using a `Vec` of `Box`ed `Chainable` items
    #[must_use]
    pub fn new(values: InnerChain<T, R>) -> Self {
        Self(Arc::new(Mutex::new(values)))
    }

    pub(crate) async fn traverse(&self, mut input: T) -> ChainResult<T, R> {
        use ChainResult::{Done, Next};
        for e in self.0.lock().await.iter_mut() {
            match e.chain(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return Done(r);
                }
            }
        }

        Next(input)
    }
}

/// TODO
#[async_trait]
pub trait Chainable<T, R>: Debug {
    /// Invoked when the current element in the `Chain` is handled.
    /// Optionally mutate `input` and return `ChainResult::Next(input)` to pass the value on onto
    /// the next element in the `Chain`.
    async fn chain(&mut self, input: T) -> ChainResult<T, R>;
}

#[derive(Debug)]
pub(crate) struct ClientRequestChain<'a> {
    chains: Vec<&'a RequestChain>,
}

impl<'a> ClientRequestChain<'a> {
    pub(crate) fn new(chains: Vec<&'a RequestChain>) -> Self {
        Self { chains }
    }

    pub(crate) async fn traverse(&self, mut input: reqwest::Request) -> Status {
        use ChainResult::{Done, Next};
        for e in &self.chains {
            match e.traverse(input).await {
                Next(r) => input = r,
                Done(r) => {
                    return r;
                }
            }
        }

        // consider as excluded if no chain element has converted it to a done
        Status::Excluded
    }
}

mod test {
    use super::{
        ChainResult,
        ChainResult::{Done, Next},
        Chainable,
    };
    use async_trait::async_trait;

    #[derive(Debug)]
    struct Add(usize);

    #[derive(Debug, PartialEq, Eq)]
    struct Result(usize);

    #[async_trait]
    impl Chainable<Result, Result> for Add {
        async fn chain(&mut self, req: Result) -> ChainResult<Result, Result> {
            let added = req.0 + self.0;
            if added > 100 {
                Done(Result(req.0))
            } else {
                Next(Result(added))
            }
        }
    }

    #[tokio::test]
    async fn simple_chain() {
        use super::Chain;
        let chain: Chain<Result, Result> = Chain::new(vec![Box::new(Add(7)), Box::new(Add(3))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Next(Result(10)));
    }

    #[tokio::test]
    async fn early_exit_chain() {
        use super::Chain;
        let chain: Chain<Result, Result> =
            Chain::new(vec![Box::new(Add(80)), Box::new(Add(30)), Box::new(Add(1))]);
        let result = chain.traverse(Result(0)).await;
        assert_eq!(result, Done(Result(80)));
    }
}
