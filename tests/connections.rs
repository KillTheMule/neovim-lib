extern crate neovim_lib;
extern crate rmp;

use neovim_lib::runtime::{Sender, channel};
use async_std::task::block_on;
use async_trait::async_trait;
use neovim_lib::{create, Handler, Requester};
use rmpv::Value;

const NVIMPATH: &str = "neovim/build/bin/nvim";

#[cfg(unix)]
use std::process::Command;
use std::process::{self, ChildStdin};

struct NH {
  pub to_main: Sender<(Value, Sender<Value>)>,
}

#[async_trait]
impl Handler for NH {
  type Writer = ChildStdin;

  async fn handle_request(
    &self,
    name: String,
    mut args: Vec<Value>,
    _req: Requester<ChildStdin>,
  ) -> Result<Value, Value> {
    eprintln!("Request: {}", name);
    match name.as_ref() {
      "dummy" => Ok(Value::from("o")),
      "req" => {
        let (sender, receiver) = channel(1);
        self.to_main.send((args.pop().unwrap(), sender)).await;
        let ret = receiver.recv().await.unwrap();
        eprintln!("Sending {}", ret.as_str().unwrap());
        Ok(ret)
      }
      _ => Ok(Value::from(2)),
    }
  }

  async fn handle_notify(
    &self,
    name: String,
    args: Vec<Value>,
    _req: Requester<ChildStdin>,
  ) {
    eprintln!("Notification: {}", name);
    match name.as_ref() {
      "not" => eprintln!("Not: {}", args[0].as_str().unwrap()),
      "quit" => {
        let (sender, _receiver) = channel(1);
        self.to_main.send((Value::from("quit"), sender)).await;
      }
      _ => {}
    };
  }
}

#[cfg(unix)]
#[test]
fn can_connect_to_child_1() {
  let rs = r#"exe ":fun M(timer) 
      call rpcrequest(1, 'req', 'y') 
    endfun""#;

  let (handler_to_main, main_from_handler) = channel(2);
  let handler = NH {
    to_main: handler_to_main,
  };

  let (nvim, fut) = create::new_child_cmd(
    Command::new(NVIMPATH)
      .args(&[
        "-u",
        "NONE",
        "--embed",
        "--headless",
        "-c",
        rs,
        "-c",
        ":let timer = timer_start(500, 'M')",
      ])
      .env("NVIM_LOG_FILE", "nvimlog"),
    handler,
  )
  .unwrap();

  let nv = nvim.requester().clone();
  let rt = nvim.runtime().clone();
  rt.spawn(async move { nv.set_var("oogle", Value::from("doodle")).await });
  rt.spawn(fut);

  block_on(async move {
    while let Some((v, c)) = main_from_handler.recv().await {
      let v = v.as_str().unwrap();
      eprintln!("Req {}", v);

      let rt = nvim.runtime().clone();
      let nvim = nvim.requester().clone();
      match v {
        "y" => rt.spawn(async move {
          let mut x: String = nvim
            .get_vvar("servername")
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .into();
          x.push_str(" - ");
          x.push_str(
            nvim.get_vvar("progname").await.unwrap().as_str().unwrap(),
          );
          x.push_str(" - ");
          x.push_str(nvim.get_var("oogle").await.unwrap().as_str().unwrap());
          x.push_str(" - ");
          x.push_str(
            nvim
              .eval("rpcrequest(1,'dummy')")
              .await
              .unwrap()
              .as_str()
              .unwrap(),
          );
          x.push_str(" - ");
          x.push_str(
            nvim
              .eval("rpcrequest(1,'req', 'z')")
              .await
              .unwrap()
              .as_str()
              .unwrap(),
          );
          c.send(Value::from(x)).await;
          nvim.command("call rpcnotify(1, 'quit')").await.unwrap();
        }),
        "z" => rt.spawn(async move {
          let x: String = nvim
            .get_vvar("progname")
            .await
            .unwrap()
            .as_str()
            .unwrap()
            .into();
          c.send(Value::from(x)).await;
        }),
        _ => break,
      };
    }
  });
  eprintln!("Quitting");
}

struct NH2 {}

#[async_trait]
impl Handler for NH2 {
  type Writer = ChildStdin;

  async fn handle_request(
    &self,
    name: String,
    args: Vec<Value>,
    req: Requester<ChildStdin>,
  ) -> Result<Value, Value> {
    eprintln!("Request: {}", name);

    match name.as_ref() {
      "dummy" => Ok(Value::from("o")),
      "req" => {
        let v = args[0].as_str().unwrap();
        eprintln!("Req {}", v);

        let req = req.clone();
        match v {
          "y" => {
            let mut x: String = req
              .get_vvar("servername")
              .await
              .unwrap()
              .as_str()
              .unwrap()
              .into();
            x.push_str(" - ");
            x.push_str(
              req.get_vvar("progname").await.unwrap().as_str().unwrap(),
            );
            x.push_str(" - ");
            x.push_str(req.get_var("oogle").await.unwrap().as_str().unwrap());
            x.push_str(" - ");
            x.push_str(
              req
                .eval("rpcrequest(1,'dummy')")
                .await
                .unwrap()
                .as_str()
                .unwrap(),
            );
            x.push_str(" - ");
            x.push_str(
              req
                .eval("rpcrequest(1,'req', 'z')")
                .await
                .unwrap()
                .as_str()
                .unwrap(),
            );
            Ok(Value::from(x))
          }
          "z" => {
            let x: String = req
              .get_vvar("progname")
              .await
              .unwrap()
              .as_str()
              .unwrap()
              .into();
            Ok(Value::from(x))
          }
          &_ => Err(Value::from("wrong argument to req")),
        }
      }
      &_ => Err(Value::from("wrong method name for request")),
    }
  }

  async fn handle_notify(
    &self,
    name: String,
    args: Vec<Value>,
    _req: Requester<ChildStdin>,
  ) {
    eprintln!("Notification: {}", name);
    match name.as_ref() {
      "not" => eprintln!("Not: {}", args[0].as_str().unwrap()),
      "quit" => {
        process::exit(0);
      }
      _ => {}
    };
  }

}

#[cfg(unix)]
#[test]
fn can_connect_to_child_2() {
  let rs = r#"exe ":fun M(timer) 
      call rpcrequest(1, 'req', 'y') 
    endfun""#;
  let rs2 = r#"exe ":fun N(timer) 
      call rpcnotify(1, 'quit') 
    endfun""#;

  let handler = NH2 {};

  let (nvim, fut) = create::new_child_cmd(
    Command::new(NVIMPATH)
      .args(&[
        "-u",
        "NONE",
        "--embed",
        "--headless",
        "-c",
        rs,
        "-c",
        ":let timer = timer_start(500, 'M')",
        "-c",
        rs2,
        "-c",
        ":let timer = timer_start(1500, 'N')",
      ])
      .env("NVIM_LOG_FILE", "nvimlog"),
    handler,
  )
  .unwrap();

  let nv = nvim.requester().clone();
  let rt = nvim.runtime().clone();
  rt.spawn(async move { nv.set_var("oogle", Value::from("doodle")).await });

  block_on(fut);

  eprintln!("Quitting");
}
