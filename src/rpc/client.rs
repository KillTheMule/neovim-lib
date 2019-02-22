use std::error::Error;
use std::io::{BufReader, BufWriter, Read, Write};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::handler::{self, DefaultHandler, Handler, RequestHandler};
use rmpv::Value;

use super::model;

type Callback = Box<FnMut(Result<Value, Value>) + Send + 'static>;
type Queue = Arc<Mutex<Vec<(u64, Sender)>>>;

enum Sender {
    Sync(mpsc::Sender<Result<Value, Value>>),
    Async(Callback),
}

impl Sender {
    fn send(self, res: Result<Value, Value>) {
        match self {
            Sender::Sync(sender) => sender.send(res).unwrap(),
            Sender::Async(mut cb) => cb(res),
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
        self.dispatch_guard
            .take()
            .expect("Can only take join handle after running event loop")
    }

    pub fn start_event_loop_channel_handler<H>(
        &mut self,
        request_handler: H,
    ) -> mpsc::Receiver<(String, Vec<Value>)>
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

    pub fn call_async(&mut self, method: String, args: Vec<Value>, cb: Option<Callback>) {
        if !self.event_loop_started {
            if let Some(mut cb) = cb {
                cb(Err(Value::from("Event loop not started")));
            } else {
                error!("Event loop not started");
            }
            return;
        }

        self.send_msg_async(method, args, cb);
    }

    pub fn call_timeout(
        &mut self,
        method: &str,
        args: Vec<Value>,
        dur: Duration,
    ) -> Result<Value, Value> {
        if !self.event_loop_started {
            return Err(Value::from("Event loop not started"));
        }

        let instant = Instant::now();
        let delay = Duration::from_millis(1);

        let receiver = self.send_msg(method, args);

        loop {
            match receiver.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {
                    thread::sleep(delay);
                    if instant.elapsed() >= dur {
                        return Err(Value::from(format!("Wait timeout ({})", method)));
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(Value::from(format!("Channel disconnected ({})", method)))
                }
                Ok(val) => return val,
            };
        }
    }

    fn send_msg_async(&mut self, method: String, params: Vec<Value>, cb: Option<Callback>) {
        let msgid = self.msgid_counter;
        self.msgid_counter += 1;

        let req = model::RpcMessage::RpcRequest {
            msgid,
            method,
            params,
        };

        if let Some(cb) = cb {
            self.queue.lock().unwrap().push((msgid, Sender::Async(cb)));
        }

        let writer = &mut *self.writer.lock().unwrap();
        model::encode(writer, req).expect("Error sending message");
    }

    fn send_msg(&mut self, method: &str, args: Vec<Value>) -> mpsc::Receiver<Result<Value, Value>> {
        let msgid = self.msgid_counter;
        self.msgid_counter += 1;

        let req = model::RpcMessage::RpcRequest {
            msgid,
            method: method.to_owned(),
            params: args,
        };

        let (sender, receiver) = mpsc::channel();
        self.queue
            .lock()
            .unwrap()
            .push((msgid, Sender::Sync(sender)));

        let writer = &mut *self.writer.lock().unwrap();
        model::encode(writer, req).expect("Error sending message");

        receiver
    }

    pub fn call(
        &mut self,
        method: &str,
        args: Vec<Value>,
        dur: Option<Duration>,
    ) -> Result<Value, Value> {
        match dur {
            Some(dur) => self.call_timeout(method, args, dur),
            None => self.call_inf(method, args),
        }
    }

    pub fn call_inf(&mut self, method: &str, args: Vec<Value>) -> Result<Value, Value> {
        if !self.event_loop_started {
            return Err(Value::from("Event loop not started"));
        }

        let receiver = self.send_msg(method, args);

        receiver.recv().unwrap()
    }

    fn send_error_to_callers(queue: &Queue, err: &Box<Error>) {
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
        mut handler: H,
    ) -> JoinHandle<()>
    where
        H: Handler + Send + 'static,
    {
        thread::spawn(move || loop {
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
                    let response = match handler.handle_request(&method, params) {
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
                    model::encode(writer, response).expect("Error sending RPC response");
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
                    handler.handle_notify(&method, params);
                }
            };
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
            let (sender, _receiver) = mpsc::channel();
            queue.lock().unwrap().push((1, Sender::Sync(sender)));
        }
        {
            let (sender, _receiver) = mpsc::channel();
            queue.lock().unwrap().push((2, Sender::Sync(sender)));
        }
        {
            let (sender, _receiver) = mpsc::channel();
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
