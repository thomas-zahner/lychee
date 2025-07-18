use http::Method;

use super::Status;

#[derive(Debug, Clone)]
pub(crate) struct RequestMethods(Vec<Method>);

impl RequestMethods {
    pub(crate) fn x(&self, x: fn(Method) -> Status) -> Status {
        todo!()
    }
}

impl Default for RequestMethods {
    fn default() -> Self {
        Self(vec![Method::GET])
    }
}
