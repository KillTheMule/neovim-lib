use std::{
  error::Error,
  io::{BufReader, BufWriter, Read, Write},
  marker::PhantomData,
  sync::{Arc, Mutex},
  thread,
  thread::JoinHandle,
  ops::AddAssign,
};

use async_std::{sync, task};

use super::handler::Handler;
use rmpv::Value;

use super::model;

type Queue = Arc<Mutex<Vec<(u64, Sender)>>>;

pub enum Sender {
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
  pub(crate) writer: Arc<Mutex<BufWriter<W>>>,
  pub(crate) queue: Queue,
  pub(crate) msgid_counter: Mutex<u64>,
  pub dispatch_guard: JoinHandle<()>,
  _p: PhantomData<R>,
}

impl<R, W> Client<R, W>
where
  R: Read + Send + 'static,
  W: Write + Send + 'static,
{
  pub fn new<H>(reader: R, writer: W, handler: H) -> Self
  where
    H: Handler + Send + 'static,
  {
    let queue = Arc::new(Mutex::new(Vec::new()));
    let writer = Arc::new(Mutex::new(BufWriter::new(writer)));
    let reader = BufReader::new(reader);

    let queue_t = queue.clone();
    let writer_t = writer.clone();

    let dispatch_guard =
      thread::spawn(move || Self::io_loop(handler, reader, queue_t, writer_t));

    Client {
      writer,
      msgid_counter: Mutex::new(0),
      queue,
      dispatch_guard,
      _p: PhantomData,
    }
  }

  fn send_msg(
    &self,
    method: &str,
    args: Vec<Value>,
  ) -> sync::Receiver<Result<Value, Value>> {
    let msgid_counter = &mut self.msgid_counter.lock().unwrap();
    let msgid = msgid_counter.clone();
    msgid_counter.add_assign(1);

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
    &self,
    method: &str,
    args: Vec<Value>,
  ) -> Result<Value, Value> {
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

  fn io_loop<H>(
    handler: H,
    mut reader: BufReader<R>,
    queue: Queue,
    writer: Arc<Mutex<BufWriter<W>>>,
  ) where
    H: Handler + Sync + 'static,
  {
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
      match msg {
        model::RpcMessage::RpcRequest {
          msgid,
          method,
          params,
        } => {
          let writer = writer.clone();
          let handler = handler.clone();
          task::spawn(async move {
            let response = match handler.handle_request(method, params).await {
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
          let handler = handler.clone();
          task::spawn(async move { handler.handle_notify(method, params).await });
        }
      };
    }
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
