use std::{
  io::{self, Error, ErrorKind, Stdout},
  net::TcpStream,
  path::Path,
  process::{ChildStdin, Command, Stdio},
};

use crate::{Handler, Neovim, Requester};

#[cfg(unix)]
use unix_socket::UnixStream;

/// Connect to nvim instance via tcp
pub fn new_tcp<H>(addr: &str, handler: H) -> io::Result<Neovim<TcpStream>>
where
  H: Handler<Writer = TcpStream> + Send + 'static,
{
  let stream = TcpStream::connect(addr)?;
  let read = stream.try_clone()?;
  let (requester, dispatch_guard) = Requester::<TcpStream>::new(stream, read, handler);

  Ok(Neovim::Tcp(requester, dispatch_guard))
}

#[cfg(unix)]
/// Connect to nvim instance via unix socket
pub fn new_unix_socket<H, P: AsRef<Path>>(
  path: P,
  handler: H,
) -> io::Result<Neovim<UnixStream>>
where
  H: Handler<Writer = UnixStream> + Send + 'static,
{
  let stream = UnixStream::connect(path)?;
  let read = stream.try_clone()?;

  let (requester, dispatch_guard) = Requester::<UnixStream>::new(stream, read, handler);

  Ok(Neovim::UnixSocket(requester, dispatch_guard))
}

/// Connect to a Neovim instance by spawning a new one.
pub fn new_child<H>(handler: H) -> io::Result<Neovim<ChildStdin>>
where
  H: Handler<Writer = ChildStdin> + Send + 'static,
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
  H: Handler<Writer = ChildStdin> + Send + 'static,
{
  new_child_cmd(Command::new(program.as_ref()).arg("--embed"), handler)
}

/// Connect to a Neovim instance by spawning a new one
///
/// stdin/stdout settings will be rewrited to `Stdio::piped()`
pub fn new_child_cmd<H>(
  cmd: &mut Command,
  handler: H,
) -> io::Result<Neovim<ChildStdin>>
where
  H: Handler<Writer = ChildStdin> + Send + 'static,
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

  let (requester, dispatch_guard) = Requester::<ChildStdin>::new(stdout, stdin, handler);

  Ok(Neovim::Child(requester, dispatch_guard, child))
}

/// Connect to a Neovim instance that spawned this process over stdin/stdout.
pub fn new_parent<H>(handler: H) -> io::Result<Neovim<Stdout>>
where
  H: Handler<Writer = Stdout> + Send + 'static,
{
  let (requester, dispatch_guard) =
    Requester::<Stdout>::new(io::stdin(), io::stdout(), handler);

  Ok(Neovim::Parent(requester, dispatch_guard))
}
