use std::{clone::Clone, io::Write, process::Child, result};

use crate::{
  callerror::{map_generic_error, CallError},
  rpc::{model::IntoVal, Requester},
  uioptions::UiAttachOptions,
};

use crate::runtime::block_on;
use rmpv::Value;

/// An active Neovim session.
pub enum Neovim<W>
where
  W: Write + Send + 'static,
{
  Child(Requester<W>, Child),
  Parent(Requester<W>),
  Tcp(Requester<W>),

  #[cfg(unix)]
  UnixSocket(Requester<W>),
}

#[macro_export]
macro_rules! call_args {
    () => (Vec::new());
    ($($e:expr), +,) => (call_args![$($e),*]);
    ($($e:expr), +) => {{
        let mut vec = Vec::new();
        $(
            vec.push($e.into_val());
        )*
        vec
    }};
}

impl<W> Neovim<W>
where
  W: Write + Send + 'static,
{
  pub fn requester(&self) -> Requester<W> {
    use Neovim::*;

    match self {
      Child(r, _) | Parent(r) | Tcp(r) => r.clone(),
      #[cfg(unix)]
      UnixSocket(r) => r.clone(),
    }
  }

  /*
  pub fn join_dispatch_guard(self) -> thread::Result<()> {
    use Neovim::*;

    match self {
      Child(_, j, _) | Parent(_, j) | Tcp(_, j) => j.join(),
      #[cfg(unix)]
      UnixSocket(_, j) => j.join(),
    }
  }
  */

  /// Call can be made only after event loop begin processing
  pub async fn call(
    &self,
    method: &str,
    args: Vec<Value>,
  ) -> result::Result<Value, Value> {
    use Neovim::*;
    match self {
      Child(r, _) | Parent(r) | Tcp(r) => r.call(method, args).await,
      #[cfg(unix)]
      UnixSocket(r) => r.call(method, args).await,
    }
  }

  /// Register as a remote UI.
  ///
  /// After this method is called, the client will receive redraw notifications.
  pub fn ui_attach(
    &mut self,
    width: i64,
    height: i64,
    opts: &UiAttachOptions,
  ) -> Result<(), CallError> {
    block_on(self.call(
      "nvim_ui_attach",
      call_args!(width, height, opts.to_value_map()),
    ))
    .map_err(map_generic_error)
    .map(|_| ())
  }

  /// Send a quit command to Nvim.
  /// The quit command is 'qa!' which will make Nvim quit without
  /// saving anything.
  pub fn quit_no_save(&mut self) -> Result<(), CallError> {
    block_on(self.requester().command("qa!"))
  }
}
