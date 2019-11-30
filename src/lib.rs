//! # Rust library for Neovim clients
//!
//! Implements support for rust plugins for [Neovim](https://github.com/neovim/neovim) through its msgpack-rpc API.
//! # Examples
//! ## Simple use case
//! ```no_run
//! use neovim_lib::{create, DefaultHandler};
//! use async_std::task;
//!
//! let mut handler = DefaultHandler{};
//! let mut nvim = create::new_tcp("127.0.0.1:6666", handler).unwrap();
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
//! use neovim_lib::{create, ChannelHandler, DefaultHandler};
//! use async_std::task;
//!
//! let mut handler = DefaultHandler{};
//! let (mut chandler, mut receiver) = ChannelHandler::new(handler);
//! let mut nvim = create::new_tcp("127.0.0.1:6666", chandler).unwrap();
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
pub mod create;

pub use crate::{
  callerror::CallError,
  neovim::Neovim,
  rpc::handler::{ChannelHandler, DefaultHandler},
  uioptions::{UiAttachOptions, UiOption},
  rpc::Requester,
};

pub use crate::rpc::handler::{Handler, RequestHandler};
pub use rmpv::{Integer, Utf8String, Value};
