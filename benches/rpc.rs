use criterion::{Criterion, criterion_group, criterion_main};
use async_std::task;
use async_trait::async_trait;
use neovim_lib::{create, Handler, call_args, rpc::IntoVal};
use std::process::{ChildStdin, Command};

const NVIMPATH: &str = "neovim/build/bin/nvim";

struct NH{}

#[async_trait]
impl Handler for NH {
  type Writer = ChildStdin;
}

fn simple_requests(c: &mut Criterion) {

  let handler = NH{};
  let (nvim, io) = create::new_child_cmd(
    Command::new(NVIMPATH)
      .args(&[
        "-u",
        "NONE",
        "--embed",
        "--headless",
      ]),
    handler,
  )
  .unwrap();

  let req = nvim.requester(); 
  task::spawn(io);

  let req1 = req.clone();
  task::block_on(async move {req1.command("set noswapfile").await}).expect("0");

  c.bench_function("simple_requests", move |b| {
    b.iter(|| {
        let req = nvim.requester();
        let _curbuf = task::block_on(async move {
          req.get_current_buf().await.expect("1");
        });
      })
    });

}

fn request_file(c: &mut Criterion) {

  let handler = NH{};
  let (nvim, io) = create::new_child_cmd(
    Command::new(NVIMPATH)
      .args(&[
        "-u",
        "NONE",
        "--embed",
        "--headless",
        "Cargo.lock"
      ]),
    handler,
  )
  .unwrap();

  let req = nvim.requester(); 
  task::spawn(io);

  let req1 = req.clone();
  task::block_on(async move {req1.command("set noswapfile").await}).expect("0");

  c.bench_function("request_file", move |b| {
    b.iter(|| {
        let req = nvim.requester();
        let _lines = task::block_on(async move {
          req.call("nvim_buf_get_lines",
            call_args![0i64, 0i64, -1i64, false]).await.expect("1");
        });
      })
    });

}

criterion_group!(name = requests; config = Criterion::default().sample_size(10).without_plots(); targets = simple_requests, request_file);
criterion_main!(requests);
