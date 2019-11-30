//! # Rust library for Neovim clients
//!
//! Implements support for rust plugins for [Neovim](https://github.com/neovim/neovim) through its msgpack-rpc API.
//! # Examples
//! ## Simple use case
//! ```no_run
//! use neovim_lib::{new_tcp, DefaultHandler};
//! use async_std::task;
//!
//! let mut handler = DefaultHandler{};
//! let mut nvim = new_tcp("127.0.0.1:6666", handler).unwrap();
//!
//! let buffers = task::block_on(nvim.requester().list_bufs()).unwrap();
//! task::block_on(buffers[0].set_lines(&mut nvim, 0, 0, true, vec!["replace first line".to_owned()])).unwrap();
//! task::block_on(nvim.requester().command("vsplit")).unwrap();
//! let windows = task::block_on(nvim.requester().list_wins()).unwrap();
//! task::block_on(windows[0].set_width(&mut nvim, 10)).unwrap();
//! ```
//! ## Process notify events from neovim
//!
//! ```no_run
//! use neovim_lib::{new_tcp, ChannelHandler, DefaultHandler};
//! use async_std::task;
//!
//! let mut handler = DefaultHandler{};
//! let (mut chandler, mut receiver) = ChannelHandler::new(handler);
//! let mut nvim = new_tcp("127.0.0.1:6666", chandler).unwrap();
//!
//! let (event_name, args) = task::block_on(receiver.recv()).unwrap();
//! ```
extern crate rmp;
extern crate rmpv;
#[macro_use]
extern crate log;

#[cfg(unix)]
extern crate unix_socket;

mod rpc;
#[macro_use]
pub mod neovim;
pub mod callerror;
pub mod neovim_api;
pub mod uioptions;

pub use crate::{
  callerror::CallError,
  neovim::Neovim,
  rpc::handler::{ChannelHandler, DefaultHandler},
  uioptions::{UiAttachOptions, UiOption},
  rpc::Requester,
};

pub use crate::rpc::handler::{Handler, RequestHandler};
pub use rmpv::{Integer, Utf8String, Value};

use std::net::TcpStream;
use std::process::{ChildStdin, Command, Stdio};
use std::io::{self, Error, ErrorKind, Stdout}; 
use std::path::Path;

#[cfg(unix)]
use unix_socket::UnixStream;

/// Connect to nvim instance via tcp
pub fn new_tcp<H>(addr: &str, handler: H) -> io::Result<Neovim<TcpStream>>
where
  H: Handler + Send + 'static,
{
  let stream = TcpStream::connect(addr)?;
  let read = stream.try_clone()?;
  let (requester, dispatch_guard) = Requester::new(stream, read, handler);

  Ok(Neovim::Tcp(requester, dispatch_guard))
}

#[cfg(unix)]
/// Connect to nvim instance via unix socket
pub fn new_unix_socket<H, P: AsRef<Path>>(
  path: P,
  handler: H,
) -> io::Result<Neovim<UnixStream>>
where
  H: Handler + Send + 'static,
{
  let stream = UnixStream::connect(path)?;
  let read = stream.try_clone()?;

  let (requester, dispatch_guard) = Requester::new(stream, read, handler);

  Ok(Neovim::UnixSocket(requester, dispatch_guard))
}

/// Connect to a Neovim instance by spawning a new one.
pub fn new_child<H>(handler: H) -> io::Result<Neovim<ChildStdin>>
where
  H: Handler + Send + 'static,
{
  if cfg!(target_os = "windows") {
    new_child_path("nvim.exe", handler)
  } else {
    new_child_path("nvim", handler)
  }
}

/// Connect to a Neovim instance by spawning a new one
pub fn new_child_path<H, S: AsRef<Path>>(
  program: S,
  handler: H,
) -> io::Result<Neovim<ChildStdin>>
where
  H: Handler + Send + 'static,
{
  new_child_cmd(Command::new(program.as_ref()).arg("--embed"), handler)
}

/// Connect to a Neovim instance by spawning a new one
///
/// stdin/stdout settings will be rewrited to `Stdio::piped()`
pub fn new_child_cmd<H>(cmd: &mut Command, handler: H) ->
  io::Result<Neovim<ChildStdin>>
where
  H: Handler + Send + 'static,
{
  let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
  let stdout = child
    .stdout
    .take()
    .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdout"))?;
  let stdin = child
    .stdin
    .take()
    .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdin"))?;

  let (requester, dispatch_guard) = Requester::new(stdout, stdin, handler);

  Ok(Neovim::Child(requester, dispatch_guard, child))
}

/// Connect to a Neovim instance that spawned this process over stdin/stdout.
pub fn new_parent<H>(handler: H) -> io::Result<Neovim<Stdout>>
where
  H: Handler + Send + 'static,
{
  let (requester, dispatch_guard) =
    Requester::new(io::stdin(), io::stdout(), handler);

  Ok(Neovim::Parent(requester, dispatch_guard))
}
