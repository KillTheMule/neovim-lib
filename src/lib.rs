//! # Rust library for Neovim clients
//!
//! Implements support for rust plugins for [Neovim](https://github.com/neovim/neovim) through its msgpack-rpc API.
//! # Examples
//! ## Simple use case
//! ```no_run
//! use neovim_lib::{Neovim, Session};
//! use async_std::task;
//!
//! let mut session = Session::new_tcp("127.0.0.1:6666").unwrap();
//! session.start_event_loop();
//! let mut nvim = Neovim::new(session);
//!
//! let buffers = task::block_on(nvim.list_bufs()).unwrap();
//! task::block_on(buffers[0].set_lines(&mut nvim, 0, 0, true, vec!["replace first line".to_owned()])).unwrap();
//! task::block_on(nvim.command("vsplit")).unwrap();
//! let windows = task::block_on(nvim.list_wins()).unwrap();
//! task::block_on(windows[0].set_width(&mut nvim, 10)).unwrap();
//! ```
//! ## Process notify events from neovim
//!
//! ```no_run
//! use neovim_lib::{Neovim, Session};
//! use async_std::task;
//! let mut session = Session::new_tcp("127.0.0.1:6666").unwrap();
//! let receiver = session.start_event_loop_channel();
//! let mut nvim = Neovim::new(session);
//!
//! let (event_name, args) = task::block_on(receiver.recv()).unwrap();
//!
//! ```
extern crate rmp;
extern crate rmpv;
#[macro_use]
extern crate log;

#[cfg(unix)]
extern crate unix_socket;

mod rpc;
#[macro_use]
pub mod session;
pub mod neovim;
pub mod neovim_api;
pub mod uioptions;
pub mod callerror;

pub use crate::neovim::Neovim;
pub use crate::uioptions::{UiAttachOptions, UiOption};
pub use crate::session::Session;
pub use crate::callerror::CallError;

pub use rmpv::{Integer, Utf8String, Value};
pub use crate::rpc::handler::{Handler, RequestHandler};
