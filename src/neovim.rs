use std::{
  io::{self, Error, ErrorKind, Stdin, Stdout},
  net::TcpStream,
  path::Path,
  process::{Child, ChildStdin, ChildStdout, Command, Stdio},
  result,
  time::Duration,
  thread::JoinHandle,
};

#[cfg(unix)]
use unix_socket::UnixStream;

use crate::{
  callerror::{map_generic_error, CallError},
  rpc::{handler::Handler, model::IntoVal, Client},
  uioptions::UiAttachOptions,
};

use async_std::task;
use rmpv::Value;

/// An active Neovim session.
pub struct Neovim {
  pub connection: ClientConnection,
  pub timeout: Option<Duration>,
  pub dispatch_guard: JoinHandle<()>,
}

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

impl Neovim {
  /// Connect to nvim instance via tcp
  pub fn new_tcp<H>(addr: &str, handler: H) -> io::Result<Neovim>
  where
    H: Handler + Send + 'static,
  {
    let stream = TcpStream::connect(addr)?;
    let read = stream.try_clone()?;
    let (client, dispatch_guard) = Client::new(stream, read, handler);
    let connection = ClientConnection::Tcp(client);

    Ok(Neovim {
      connection,
      timeout: Some(Duration::new(5, 0)),
      dispatch_guard,
    })
  }

  #[cfg(unix)]
  /// Connect to nvim instance via unix socket
  pub fn new_unix_socket<H, P: AsRef<Path>>(
    path: P,
    handler: H,
  ) -> io::Result<Neovim>
  where
    H: Handler + Send + 'static,
  {
    let stream = UnixStream::connect(path)?;
    let read = stream.try_clone()?;

    let (client, dispatch_guard) = Client::new(stream, read, handler);
    let connection = ClientConnection::UnixSocket(client);

    Ok(Neovim {
      connection,
      timeout: Some(Duration::new(5, 0)),
      dispatch_guard,
    })
  }

  /// Connect to a Neovim instance by spawning a new one.
  pub fn new_child<H>(handler: H) -> io::Result<Neovim>
  where
    H: Handler + Send + 'static,
  {
    if cfg!(target_os = "windows") {
      Self::new_child_path("nvim.exe", handler)
    } else {
      Self::new_child_path("nvim", handler)
    }
  }

  /// Connect to a Neovim instance by spawning a new one
  pub fn new_child_path<H, S: AsRef<Path>>(
    program: S,
    handler: H,
  ) -> io::Result<Neovim>
  where
    H: Handler + Send + 'static,
  {
    Self::new_child_cmd(Command::new(program.as_ref()).arg("--embed"), handler)
  }

  /// Connect to a Neovim instance by spawning a new one
  ///
  /// stdin/stdout settings will be rewrited to `Stdio::piped()`
  pub fn new_child_cmd<H>(cmd: &mut Command, handler: H) -> io::Result<Neovim>
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

    let (client, dispatch_guard) = Client::new(stdout, stdin, handler);
    let connection = ClientConnection::Child(client, child);

    Ok(Neovim {
      connection,
      timeout: Some(Duration::new(5, 0)),
      dispatch_guard,
    })
  }

  /// Connect to a Neovim instance that spawned this process over stdin/stdout.
  pub fn new_parent<H>(handler: H) -> io::Result<Neovim>
  where
    H: Handler + Send + 'static,
  {
    let (client, dispatch_guard) = Client::new(io::stdin(), io::stdout(), handler);
    let connection = ClientConnection::Parent(client);

    Ok(Neovim {
      connection,
      timeout: Some(Duration::new(5, 0)),
      dispatch_guard,
    })
  }

  /// Set call timeout
  pub fn set_timeout(&mut self, timeout: Duration) {
    self.timeout = Some(timeout);
  }

  pub fn set_infinity_timeout(&mut self) {
    self.timeout = None;
  }

  /// Call can be made only after event loop begin processing
  pub async fn call(
    &self,
    method: &str,
    args: Vec<Value>,
  ) -> result::Result<Value, Value> {
    match self.connection {
      ClientConnection::Child(ref client, _) => {
        client.call(method, args).await
      }
      ClientConnection::Parent(ref client) => {
        client.call(method, args).await
      }
      ClientConnection::Tcp(ref client) => client.call(method, args).await,

      #[cfg(unix)]
      ClientConnection::UnixSocket(ref client) => {
        client.call(method, args).await
      }
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
    task::block_on(self.call(
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
    task::block_on(self.command("qa!"))
  }
}

pub enum ClientConnection {
  Child(Client<ChildStdout, ChildStdin>, Child),
  Parent(Client<Stdin, Stdout>),
  Tcp(Client<TcpStream, TcpStream>),

  #[cfg(unix)]
  UnixSocket(Client<UnixStream, UnixStream>),
}
