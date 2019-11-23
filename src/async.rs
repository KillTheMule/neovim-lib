use std::marker::PhantomData;

use rmpv::Value;

use crate::neovim;
use crate::rpc::model::FromVal;
use crate::session::ClientConnection;

pub struct AsyncCall<'a, R: FromVal<Value>> {
    method: String,
    args: Vec<Value>,
    client: &'a mut ClientConnection,
    cb: Option<Box<dyn FnMut(Result<Value, Value>) + Send + 'static>>,
    marker: PhantomData<R>,
}

impl<'a, R: FromVal<Value>> AsyncCall<'a, R> {
    pub fn new(client: &'a mut ClientConnection, method: String, args: Vec<Value>) -> Self {
        AsyncCall {
            method,
            args,
            client,
            cb: None,
            marker: PhantomData,
        }
    }

    pub fn cb<F>(mut self, cb: F) -> Self
    where
        F: FnOnce(Result<R, neovim::CallError>) + Send + 'static,
    {
        let mut cb = Some(cb);

        self.cb = Some(Box::new(move |res| {
            let res = res.map(R::from_val).map_err(neovim::map_generic_error);
            cb.take().unwrap()(res);
        }));
        self
    }

    /// Async call. Call can be made only after event loop begin processing
    pub fn call(self) {
        match *self.client {
            ClientConnection::Child(ref mut client, _) => {
                client.call_async(self.method, self.args, self.cb)
            }
            ClientConnection::Parent(ref mut client) => {
                client.call_async(self.method, self.args, self.cb)
            }
            ClientConnection::Tcp(ref mut client) => {
                client.call_async(self.method, self.args, self.cb)
            }

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => {
                client.call_async(self.method, self.args, self.cb)
            }
        };
    }
}
