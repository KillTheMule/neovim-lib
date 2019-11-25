use std::{
  error::Error,
  io::{BufReader, BufWriter, Read, Write},
  sync::{Arc, Mutex},
  thread,
  thread::JoinHandle,
};

use async_std::{sync, task};

use super::handler::{self, DefaultHandler, Handler, RequestHandler};
use rmpv::Value;

use super::model;

type Queue = Arc<Mutex<Vec<(u64, Sender)>>>;

enum Sender {
  Sync(sync::Sender<Result<Value, Value>>),
}

impl Sender {
  fn send(self, res: Result<Value, Value>) {
    match self {
      Sender::Sync(sender) => task::block_on(sender.send(res)),
    };
  }
}

pub struct Client<R, W>
where
  R: Read + Send + 'static,
  W: Write + Send + 'static,
{
  reader: Option<BufReader<R>>,
  writer: Arc<Mutex<BufWriter<W>>>,
  dispatch_guard: Option<JoinHandle<()>>,
  event_loop_started: bool,
  queue: Queue,
  msgid_counter: u64,
}

impl<R, W> Client<R, W>
where
  R: Read + Send + 'static,
  W: Write + Send + 'static,
{
  pub fn take_dispatch_guard(&mut self) -> JoinHandle<()> {
    self
      .dispatch_guard
      .take()
      .expect("Can only take join handle after running event loop")
  }

  pub fn start_event_loop_channel_handler<H>(
    &mut self,
    request_handler: H,
  ) -> sync::Receiver<(String, Vec<Value>)>
  where
    H: RequestHandler + Send + 'static,
  {
    let (handler, reciever) = handler::channel(request_handler);

    self.dispatch_guard = Some(Self::dispatch_thread(
      self.queue.clone(),
      self.reader.take().unwrap(),
      self.writer.clone(),
      handler,
    ));
    self.event_loop_started = true;

    reciever
  }

  pub fn start_event_loop_handler<H>(&mut self, handler: H)
  where
    H: Handler + Send + 'static,
  {
    self.dispatch_guard = Some(Self::dispatch_thread(
      self.queue.clone(),
      self.reader.take().unwrap(),
      self.writer.clone(),
      handler,
    ));
    self.event_loop_started = true;
  }

  pub fn start_event_loop(&mut self) {
    self.dispatch_guard = Some(Self::dispatch_thread(
      self.queue.clone(),
      self.reader.take().unwrap(),
      self.writer.clone(),
      DefaultHandler(),
    ));
    self.event_loop_started = true;
  }

  pub fn new(reader: R, writer: W) -> Self {
    let queue = Arc::new(Mutex::new(Vec::new()));
    Client {
      reader: Some(BufReader::new(reader)),
      writer: Arc::new(Mutex::new(BufWriter::new(writer))),
      msgid_counter: 0,
      queue: queue.clone(),
      dispatch_guard: None,
      event_loop_started: false,
    }
  }

  fn send_msg(
    &mut self,
    method: &str,
    args: Vec<Value>,
  ) -> sync::Receiver<Result<Value, Value>> {
    let msgid = self.msgid_counter;
    self.msgid_counter += 1;

    let req = model::RpcMessage::RpcRequest {
      msgid,
      method: method.to_owned(),
      params: args,
    };

    let (sender, receiver) = sync::channel(1);

    self
      .queue
      .lock()
      .unwrap()
      .push((msgid, Sender::Sync(sender)));

    let writer = &mut *self.writer.lock().unwrap();
    model::encode(writer, req).expect("Error sending message");

    receiver
  }

  pub async fn call(
    &mut self,
    method: &str,
    args: Vec<Value>,
  ) -> Result<Value, Value> {
    if !self.event_loop_started {
      return Err(Value::from("Event loop not started"));
    }

    let receiver = self.send_msg(method, args);

    receiver.recv().await.unwrap_or_else(|| {
      Err(Value::from(format!(
        "Method '{}' did not receive a response",
        method
      )))
    })
  }

  fn send_error_to_callers(queue: &Queue, err: &Box<dyn Error>) {
    let mut queue = queue.lock().unwrap();
    queue.drain(0..).for_each(|sender| {
      sender
        .1
        .send(Err(Value::from(format!("Error read response: {}", err))))
    });
  }

  fn dispatch_thread<H>(
    queue: Queue,
    mut reader: BufReader<R>,
    writer: Arc<Mutex<BufWriter<W>>>,
    handler: H,
  ) -> JoinHandle<()>
  where
    H: Handler + Sync + 'static,
  {
    thread::spawn(move || {
      let handler = Arc::new(handler);
      loop {
        let msg = match model::decode(&mut reader) {
          Ok(msg) => msg,
          Err(e) => {
            error!("Error while reading: {}", e);
            Self::send_error_to_callers(&queue, &e);
            return;
          }
        };
        debug!("Get message {:?}", msg);
        let h1 = handler.clone();
        let writer = writer.clone();
        match msg {
          model::RpcMessage::RpcRequest {
            msgid,
            method,
            params,
          } => {
            task::spawn(async move {
              let response = match h1.handle_request(method, params).await {
                Ok(result) => model::RpcMessage::RpcResponse {
                  msgid,
                  result,
                  error: Value::Nil,
                },
                Err(error) => model::RpcMessage::RpcResponse {
                  msgid,
                  result: Value::Nil,
                  error,
                },
              };

              let writer = &mut *writer.lock().unwrap();
              model::encode(writer, response)
                .expect("Error sending RPC response");
            });
          }
          model::RpcMessage::RpcResponse {
            msgid,
            result,
            error,
          } => {
            let sender = find_sender(&queue, msgid);
            if error != Value::Nil {
              sender.send(Err(error));
            } else {
              sender.send(Ok(result));
            }
          }
          model::RpcMessage::RpcNotification { method, params } => {
            task::spawn(async move { h1.handle_notify(method, params).await });
          }
        };
      }
    })
  }
}

/* The idea to use Vec here instead of HashMap
 * is that Vec is faster on small queue sizes
 * in most cases Vec.len = 1 so we just take first item in iteration.
 */
fn find_sender(queue: &Queue, msgid: u64) -> Sender {
  let mut queue = queue.lock().unwrap();

  let pos = queue.iter().position(|req| req.0 == msgid).unwrap();
  queue.remove(pos).1
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_find_sender() {
    let queue = Arc::new(Mutex::new(Vec::new()));

    {
      let (sender, _receiver) = sync::channel(1);
      queue.lock().unwrap().push((1, Sender::Sync(sender)));
    }
    {
      let (sender, _receiver) = sync::channel(1);
      queue.lock().unwrap().push((2, Sender::Sync(sender)));
    }
    {
      let (sender, _receiver) = sync::channel(1);
      queue.lock().unwrap().push((3, Sender::Sync(sender)));
    }

    find_sender(&queue, 1);
    assert_eq!(2, queue.lock().unwrap().len());
    find_sender(&queue, 2);
    assert_eq!(1, queue.lock().unwrap().len());
    find_sender(&queue, 3);
    assert!(queue.lock().unwrap().is_empty());
  }
}
