//! # Rust library for Neovim clients
//!
//! Implements support for rust plugins for [Neovim](https://github.com/neovim/neovim) through its msgpack-rpc API.
//! # Examples
//! ## Simple use case
//! ```no_run
//! use neovim_lib::{create, DefaultHandler};
//! use async_std::task;
//!
//! let mut handler = DefaultHandler::new();
//! let (nvim, _) = create::new_tcp("127.0.0.1:6666", handler).unwrap();
//!
//! let buffers = task::block_on(nvim.list_bufs()).unwrap();
//! task::block_on(buffers[0].set_lines(&nvim, 0, 0, true, vec!["replace first line".to_owned()])).unwrap();
//! task::block_on(nvim.command("vsplit")).unwrap();
//! let windows = task::block_on(nvim.list_wins()).unwrap();
//! task::block_on(windows[0].set_width(&nvim, 10)).unwrap();
//! ```
//! ## Process notify events from neovim
//!
//! ```no_run
//! use neovim_lib::{create, Handler, Value, Requester};
//! use async_std::{task, sync};
//! use async_trait::async_trait;
//! use std::net::TcpStream;
//!
//! struct MyHandler(sync::Sender<(String, Vec<Value>)>);
//!
//! #[async_trait]
//! impl Handler for MyHandler {
//!   type Writer = TcpStream;
//!
//!   async fn handle_notify(
//!     &self,
//!     name: String,
//!     args: Vec<Value>,
//!     _: Requester<TcpStream>
//!   ) {
//!     self.0.send((name, args)).await;
//!   }
//! }
//!
//! let (mut sender, mut receiver) = sync::channel(1);
//! let mut handler = MyHandler(sender);
//! let mut nvim = create::new_tcp("127.0.0.1:6666", handler).unwrap();
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
pub mod create;
pub mod neovim_api;
pub mod uioptions;

pub use crate::{
  callerror::CallError,
  neovim::Neovim,
  rpc::{handler::DefaultHandler, Requester},
  uioptions::{UiAttachOptions, UiOption},
};

pub use crate::rpc::handler::Handler;
pub use rmpv::{Integer, Utf8String, Value};
