use std::{io::Write, marker::PhantomData, sync::Arc};

use async_std::sync;
use async_trait::async_trait;
use rmpv::Value;

use crate::Requester;

#[async_trait]
pub trait RequestHandler: Sync + Send {
  type Writer: Write + Send + 'static;

  async fn handle_request(
    &self,
    _name: String,
    _args: Vec<Value>,
    _req: Requester<Self::Writer>,
  ) -> Result<Value, Value> {
    Err(Value::from("Not implemented"))
  }
}

#[async_trait]
pub trait Handler: RequestHandler {
  async fn handle_notify(&self, _name: String, _args: Vec<Value>, _req:
    Requester<<Self as RequestHandler>::Writer>)
    {}
}

pub struct DefaultHandler<Q>
where
    Q: Write + Send + Sync + 'static,
{
  _q: Arc<PhantomData<Q>>
}

impl<Q> RequestHandler for DefaultHandler<Q> 
where
    Q: Write + Send + Sync + 'static,
  {
  type Writer = Q;
}

impl<Q> Handler for DefaultHandler<Q>
where
    Q: Write + Send + Sync + 'static,
  {}

impl<Q> DefaultHandler<Q>
where
    Q: Write + Send + Sync + 'static,
{
  pub fn new() -> DefaultHandler<Q> {
    DefaultHandler {
      _q: Arc::new(PhantomData)
    }
  }
}

pub struct ChannelHandler<H: RequestHandler> {
  sender: sync::Sender<(String, Vec<Value>)>,
  request_handler: H,
}

#[async_trait]
impl<H: RequestHandler> Handler for ChannelHandler<H> {
  async fn handle_notify(&self, name: String, args: Vec<Value>, _req:
    Requester<H::Writer>) {
    self.sender.send((name.to_owned(), args)).await
  }
}

#[async_trait]
impl<H: RequestHandler> RequestHandler for ChannelHandler<H> {
  type Writer = H::Writer;

  async fn handle_request(
    &self,
    name: String,
    args: Vec<Value>,
    req: Requester<<H as RequestHandler>::Writer>,
  ) -> Result<Value, Value> {
    (&*self).request_handler.handle_request(name, args, req).await
  }
}

impl<H: RequestHandler> ChannelHandler<H> {
  pub fn new(
    request_handler: H,
  ) -> (Self, sync::Receiver<(String, Vec<Value>)>) {
    let (sender, receiver) = sync::channel(10); //TODO: Is 10 a good number?
    (
      ChannelHandler {
        request_handler,
        sender,
      },
      receiver,
    )
  }
}
