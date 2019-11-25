use std::io::{self, Error, ErrorKind, Stdin, Stdout};
use std::net::TcpStream;
use std::process::Stdio;
use std::process::{Child, ChildStdin, ChildStdout, Command};
use std::result;
use std::thread::JoinHandle;
use std::time::Duration;

use std::path::Path;
#[cfg(unix)]
use unix_socket::UnixStream;

use crate::rpc::handler::{DefaultHandler, Handler, RequestHandler};
use crate::rpc::Client;
use crate::uioptions::UiAttachOptions;
use crate::callerror::{CallError, map_generic_error};
use crate::rpc::model::IntoVal;

use async_std::{sync, task};
use rmpv::Value;

/// An active Neovim session.
pub struct Neovim {
    client: ClientConnection,
    timeout: Option<Duration>,
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
    pub fn new_tcp(addr: &str) -> io::Result<Neovim> {
        let stream = TcpStream::connect(addr)?;
        let read = stream.try_clone()?;
        Ok(Neovim {
            client: ClientConnection::Tcp(Client::new(stream, read)),
            timeout: Some(Duration::new(5, 0)),
        })
    }

    #[cfg(unix)]
    /// Connect to nvim instance via unix socket
    pub fn new_unix_socket<P: AsRef<Path>>(path: P) -> io::Result<Neovim> {
        let stream = UnixStream::connect(path)?;
        let read = stream.try_clone()?;
        Ok(Neovim {
            client: ClientConnection::UnixSocket(Client::new(stream, read)),
            timeout: Some(Duration::new(5, 0)),
        })
    }

    /// Connect to a Neovim instance by spawning a new one.
    pub fn new_child() -> io::Result<Neovim> {
        if cfg!(target_os = "windows") {
            Self::new_child_path("nvim.exe")
        } else {
            Self::new_child_path("nvim")
        }
    }

    /// Connect to a Neovim instance by spawning a new one
    pub fn new_child_path<S: AsRef<Path>>(program: S) -> io::Result<Neovim> {
        Self::new_child_cmd(Command::new(program.as_ref()).arg("--embed"))
    }

    /// Connect to a Neovim instance by spawning a new one
    ///
    /// stdin/stdout settings will be rewrited to `Stdio::piped()`
    pub fn new_child_cmd(cmd: &mut Command) -> io::Result<Neovim> {
        let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdout"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::new(ErrorKind::Other, "Can't open stdin"))?;

        Ok(Neovim {
            client: ClientConnection::Child(Client::new(stdout, stdin), child),
            timeout: Some(Duration::new(5, 0)),
        })
    }

    /// Connect to a Neovim instance that spawned this process over stdin/stdout.
    pub fn new_parent() -> io::Result<Neovim> {
        Ok(Neovim {
            client: ClientConnection::Parent(Client::new(io::stdin(), io::stdout())),
            timeout: Some(Duration::new(5, 0)),
        })
    }

    /// Set call timeout
    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = Some(timeout);
    }

    pub fn set_infinity_timeout(&mut self) {
        self.timeout = None;
    }

    /// Start processing rpc response and notifications
    pub fn start_event_loop_channel_handler<H>(
        &mut self,
        request_handler: H,
    ) -> sync::Receiver<(String, Vec<Value>)>
    where
        H: RequestHandler + Send + 'static,
    {
        match self.client {
            ClientConnection::Child(ref mut client, _) => {
                client.start_event_loop_channel_handler(request_handler)
            }
            ClientConnection::Parent(ref mut client) => {
                client.start_event_loop_channel_handler(request_handler)
            }
            ClientConnection::Tcp(ref mut client) => {
                client.start_event_loop_channel_handler(request_handler)
            }

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => {
                client.start_event_loop_channel_handler(request_handler)
            }
        }
    }

    /// Start processing rpc response and notifications
    pub fn start_event_loop_channel(&mut self) -> sync::Receiver<(String, Vec<Value>)> {
        self.start_event_loop_channel_handler(DefaultHandler())
    }

    /// Start processing rpc response and notifications
    pub fn start_event_loop_handler<H>(&mut self, handler: H)
    where
        H: Handler + Send + 'static,
    {
        match self.client {
            ClientConnection::Child(ref mut client, _) => client.start_event_loop_handler(handler),
            ClientConnection::Parent(ref mut client) => client.start_event_loop_handler(handler),
            ClientConnection::Tcp(ref mut client) => client.start_event_loop_handler(handler),

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => {
                client.start_event_loop_handler(handler)
            }
        }
    }

    /// Start processing rpc response and notifications
    pub fn start_event_loop(&mut self) {
        match self.client {
            ClientConnection::Child(ref mut client, _) => client.start_event_loop(),
            ClientConnection::Parent(ref mut client) => client.start_event_loop(),
            ClientConnection::Tcp(ref mut client) => client.start_event_loop(),

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => client.start_event_loop(),
        }
    }

    /// Call can be made only after event loop begin processing
    pub async fn call(&mut self, method: &str, args: Vec<Value>) -> result::Result<Value, Value> {
        match self.client {
            ClientConnection::Child(ref mut client, _) => client.call(method, args).await,
            ClientConnection::Parent(ref mut client) => client.call(method, args).await,
            ClientConnection::Tcp(ref mut client) => client.call(method, args).await,

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => client.call(method, args).await,
        }
    }

    /// Wait dispatch thread to finish.
    ///
    /// This can happens in case child process connection is lost for some reason.
    pub fn take_dispatch_guard(&mut self) -> JoinHandle<()> {
        match self.client {
            ClientConnection::Child(ref mut client, _) => client.take_dispatch_guard(),
            ClientConnection::Parent(ref mut client) => client.take_dispatch_guard(),
            ClientConnection::Tcp(ref mut client) => client.take_dispatch_guard(),

            #[cfg(unix)]
            ClientConnection::UnixSocket(ref mut client) => client.take_dispatch_guard(),
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
        task::block_on(self
            .call(
                "nvim_ui_attach",
                call_args!(width, height, opts.to_value_map()),
            )
        )
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
