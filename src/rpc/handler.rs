use rmpv::Value;
use std::boxed::Box;
use std::error::Error;
use std::sync::mpsc;

pub trait RequestHandler {
    fn handle_request(
        &mut self,
        _name: &str,
        _args: Vec<Value>,
        f: Box<dyn FnOnce(Result<Value, Value>) -> Result<(), Box<Error>> + Send
        + 'static>
        ) {
        unimplemented!()
    }
}

pub trait Handler: RequestHandler {
    fn handle_notify(&mut self, _name: &str, _args: Vec<Value>) {}
}

pub struct DefaultHandler();

impl RequestHandler for DefaultHandler {}
impl Handler for DefaultHandler {}

pub struct ChannelHandler<H: RequestHandler> {
    sender: mpsc::Sender<(String, Vec<Value>)>,
    request_handler: H,
}

impl<H: RequestHandler> Handler for ChannelHandler<H> {
    fn handle_notify(&mut self, name: &str, args: Vec<Value>) {
        self.sender.send((name.to_owned(), args)).unwrap()
    }
}

impl<H: RequestHandler> RequestHandler for ChannelHandler<H> {
    fn handle_request(
        &mut self,
        name: &str,
        args: Vec<Value>,
        f: Box<dyn FnOnce(Result<Value, Value>) -> Result<(), Box<Error>> + Send
        + 'static>
    ) {
        self.request_handler.handle_request(name, args, f)
    }
}

impl<H: RequestHandler> ChannelHandler<H> {
    pub fn new(request_handler: H) -> (Self, mpsc::Receiver<(String, Vec<Value>)>) {
        let (sender, receiver) = mpsc::channel();
        (
            ChannelHandler {
                request_handler,
                sender,
            },
            receiver,
        )
    }
}

pub fn channel<H: RequestHandler>(
    request_handler: H,
) -> (ChannelHandler<H>, mpsc::Receiver<(String, Vec<Value>)>) {
    ChannelHandler::new(request_handler)
}
