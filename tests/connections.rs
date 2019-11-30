extern crate neovim_lib;
extern crate rmp;

use async_std::{sync, task};
use async_trait::async_trait;
use neovim_lib::{Handler, RequestHandler, new_child_cmd};
use rmpv::Value;

struct NH {
  pub to_main: sync::Sender<(Value, sync::Sender<Value>)>,
}

#[async_trait]
impl Handler for NH {
  async fn handle_notify(&self, name: String, args: Vec<Value>) {
    eprintln!("Notification: {}", name);
    match name.as_ref() {
      "not" => eprintln!("Not: {}", args[0].as_str().unwrap()),
      "quit" => {
          let (sender, _receiver) = sync::channel(1);
          self.to_main.send((Value::from("quit"), sender)).await;
        }
      _ => {}
    };
  }
}

#[async_trait]
impl RequestHandler for NH {
  async fn handle_request(
    &self,
    name: String,
    mut args: Vec<Value>,
  ) -> Result<Value, Value> {
    eprintln!("Request: {}", name);
    match name.as_ref() {
      "dummy" => Ok(Value::from("o")),
      "req" => {
        let (sender, receiver) = sync::channel(1);
        self.to_main.send((args.pop().unwrap(), sender)).await;
        let ret = receiver.recv().await.unwrap();
        eprintln!("Sending {}", ret.as_str().unwrap());
        Ok(ret)
      }
      _ => Ok(Value::from(2)),
    }
  }
}

#[cfg(unix)]
use std::process::Command;

#[cfg(unix)]
#[test]
fn can_connect_to_child() {
  let nvimpath = "/home/pips/Devel/neovim/neovim/build/bin/nvim";
  let rs = r#"exe ":fun M(timer) 
      call rpcrequest(1, 'req', 'y') 
    endfun""#;

  let (handler_to_main, main_from_handler) = sync::channel(2);
  let handler = NH {
    to_main: handler_to_main,
  };

  let nvim = new_child_cmd(
    Command::new(nvimpath)
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
      .env("VIMRUNTIME", "/home/pips/Devel/neovim/neovim/runtime")
      .env("NVIM_LOG_FILE", "nvimlog"),
    handler,
  )
  .unwrap();

  let nv = nvim.requester().clone();

  task::spawn(async move { nv.set_var("oogle", Value::from("doodle")).await });

  task::block_on(async move {
    'w: while let Some((v, c)) = main_from_handler.recv().await {
      let v = v.as_str().unwrap();
      eprintln!("Req {}", v);

      let nvim = nvim.requester().clone();
      match v {
        "y" => task::spawn(async move {
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
          x.push_str(nvim.eval("rpcrequest(1,'dummy')").await.unwrap().as_str().unwrap());
          x.push_str(" - ");
          x.push_str(nvim.eval("rpcrequest(1,'req', 'z')").await.unwrap().as_str().unwrap());
          c.send(Value::from(x)).await;
          nvim.command("call rpcnotify(1, 'quit')").await.unwrap();
        }),
        "z" => task::spawn(async move {
          let x:String =
            nvim.get_vvar("progname").await.unwrap().as_str().unwrap().into();
          c.send(Value::from(x)).await;
        }),
        _ => break,
      };
    }
  });
  eprintln!("Quitting");
}
