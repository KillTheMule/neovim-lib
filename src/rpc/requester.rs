use std::{
  error::Error,
  io::{BufReader, BufWriter, Read, Write},
  sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
  },
  thread,
  thread::JoinHandle,
};

use async_std::{sync, task};

use crate::rpc::{model, handler::Handler};
use rmpv::Value;

type Queue = Arc<Mutex<Vec<(u64, sync::Sender<Result<Value, Value>>)>>>;

pub struct Requester<W>
where
  W: Write + Send + 'static,
{
  pub(crate) writer: Arc<Mutex<BufWriter<W>>>,
  pub(crate) queue: Queue,
  pub(crate) msgid_counter: Arc<AtomicU64>,
}

impl<W> Clone for Requester<W>
where
  W: Write + Send + 'static,
{
  fn clone(&self) -> Self {
    Requester {
      writer: self.writer.clone(),
      queue: self.queue.clone(),
      msgid_counter: self.msgid_counter.clone(),
    }
  }
}

impl<W> Requester<W>
where
  W: Write + Send + 'static,
{
  pub fn new<H, R>(
    reader: R,
    writer: <H as Handler>::Writer,
    handler: H,
  ) -> (Requester<<H as Handler>::Writer>, JoinHandle<()>)
  where
    R: Read + Send + 'static,
    H: Handler + Send + 'static,
  {
    let reader = BufReader::new(reader);

    let req = Requester {
      writer: Arc::new(Mutex::new(BufWriter::new(writer))),
      msgid_counter: Arc::new(AtomicU64::new(0)),
      queue: Arc::new(Mutex::new(Vec::new())),
    };

    let req_t = req.clone();

    let dispatch_guard =
      thread::spawn(move || Self::io_loop(handler, reader, req_t));

    (req, dispatch_guard)
  }

  fn send_msg(
    &self,
    method: &str,
    args: Vec<Value>,
  ) -> sync::Receiver<Result<Value, Value>> {
    let msgid = self.msgid_counter.fetch_add(1, Ordering::SeqCst);

    let req = model::RpcMessage::RpcRequest {
      msgid,
      method: method.to_owned(),
      params: args,
    };

    let (sender, receiver) = sync::channel(1);

    self.queue.lock().unwrap().push((msgid, sender));

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
      let e = format!("Error read response: {}", err);
      task::spawn(async move { sender.1.send(Err(Value::from(e))).await });
    });
  }

  fn io_loop<H, R>(
    handler: H,
    mut reader: BufReader<R>,
    req: Requester<<H as Handler>::Writer>,
  ) where
    H: Handler + Sync + 'static,
    R: Read + Send + 'static,
  {
    let handler = Arc::new(handler);
    loop {
      let msg = match model::decode(&mut reader) {
        Ok(msg) => msg,
        Err(e) => {
          error!("Error while reading: {}", e);
          Self::send_error_to_callers(&req.queue, &e);
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
          let req = req.clone();
          let handler = handler.clone();
          task::spawn(async move {
            let req_t = req.clone();
            let response =
              match handler.handle_request(method, params, req_t).await {
                Ok(result) => {
                  let r = model::RpcMessage::RpcResponse {
                    msgid,
                    result,
                    error: Value::Nil,
                  };
                  r
                }
                Err(error) => model::RpcMessage::RpcResponse {
                  msgid,
                  result: Value::Nil,
                  error,
                },
              };

            let writer = &mut *(req.writer).lock().unwrap();
            model::encode(writer, response)
              .expect("Error sending RPC response");
          });
        }
        model::RpcMessage::RpcResponse {
          msgid,
          result,
          error,
        } => {
          let sender = find_sender(&req.queue, msgid);
          if error != Value::Nil {
            task::spawn(async move {
              sender.send(Err(error)).await;
            });
          } else {
            task::spawn(async move {
              sender.send(Ok(result)).await;
            });
          }
        }
        model::RpcMessage::RpcNotification { method, params } => {
          let handler = handler.clone();
          let req = req.clone();
          task::spawn(async move {
            handler.handle_notify(method, params, req).await
          });
        }
      };
    }
  }
}

/* The idea to use Vec here instead of HashMap
 * is that Vec is faster on small queue sizes
 * in most cases Vec.len = 1 so we just take first item in iteration.
 */
fn find_sender(
  queue: &Queue,
  msgid: u64,
) -> sync::Sender<Result<Value, Value>> {
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
      queue.lock().unwrap().push((1, sender));
    }
    {
      let (sender, _receiver) = sync::channel(1);
      queue.lock().unwrap().push((2, sender));
    }
    {
      let (sender, _receiver) = sync::channel(1);
      queue.lock().unwrap().push((3, sender));
    }

    find_sender(&queue, 1);
    assert_eq!(2, queue.lock().unwrap().len());
    find_sender(&queue, 2);
    assert_eq!(1, queue.lock().unwrap().len());
    find_sender(&queue, 3);
    assert!(queue.lock().unwrap().is_empty());
  }
}
