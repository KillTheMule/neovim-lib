extern crate neovim_lib;
extern crate rmp;

use async_std::{sync, task};
use async_trait::async_trait;
use neovim_lib::{neovim::Neovim, Handler, RequestHandler};
use rmpv::Value;

struct NH {
  pub to_main: sync::Sender<Value>,
  pub from_main: sync::Receiver<Value>,
}

#[async_trait]
impl Handler for NH {
  async fn handle_notify(&self, name: String, args: Vec<Value>) {
    eprintln!("Notification: {}", name);
    match name.as_ref() {
      "not" => eprintln!("Not: {}", args[0].as_str().unwrap()),
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
        self.to_main.send(args.pop().unwrap()).await;
        let ret = self.from_main.recv().await.unwrap();
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
  let (main_to_handler, handler_from_main) = sync::channel(2);
  let handler = NH {
    to_main: handler_to_main,
    from_main: handler_from_main,
  };

  let mut nvim = Neovim::new_child_cmd(
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

  task::block_on(async move {
    while let Some(v) = main_from_handler.recv().await {
      eprintln!("Req {}", v.as_str().unwrap());
      let mut x: String = nvim
        .get_vvar("servername")
        .await
        .unwrap()
        .as_str()
        .unwrap()
        .into();
      x.push_str(" - ");
      x.push_str(nvim.get_vvar("progname").await.unwrap().as_str().unwrap());
      main_to_handler.send(Value::from(x)).await;
      break;
    }
  });
  eprintln!("Quitting");
}
