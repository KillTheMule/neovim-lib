use async_std::sync;
use async_trait::async_trait;
use rmpv::Value;

#[async_trait]
pub trait RequestHandler: Sync + Send {
  async fn handle_request(
    &self,
    _name: String,
    _args: Vec<Value>,
  ) -> Result<Value, Value> {
    Err(Value::from("Not implemented"))
  }
}

#[async_trait]
pub trait Handler: RequestHandler {
  async fn handle_notify(&self, _name: String, _args: Vec<Value>) {}
}

pub struct DefaultHandler();

impl RequestHandler for DefaultHandler {}
impl Handler for DefaultHandler {}

pub struct ChannelHandler<H: RequestHandler> {
  sender: sync::Sender<(String, Vec<Value>)>,
  request_handler: H,
}

#[async_trait]
impl<H: RequestHandler> Handler for ChannelHandler<H> {
  async fn handle_notify(&self, name: String, args: Vec<Value>) {
    self.sender.send((name.to_owned(), args)).await
  }
}

#[async_trait]
impl<H: RequestHandler> RequestHandler for ChannelHandler<H> {
  async fn handle_request(
    &self,
    name: String,
    args: Vec<Value>,
  ) -> Result<Value, Value> {
    (&*self).request_handler.handle_request(name, args).await
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
