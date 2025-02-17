// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

fn main() -> wry::Result<()> {
  use wry::{
    application::{
      event::{Event, WindowEvent},
      event_loop::{ControlFlow, EventLoop},
      window::{Window, WindowBuilder},
    },
    webview::{RpcRequest, WebViewBuilder},
  };

  let event_loop = EventLoop::new();
  let window = WindowBuilder::new()
    .with_decorations(false)
    .build(&event_loop)
    .unwrap();

  let url = r#"data:text/html,
        <body>
          <div class='drag-region titlebar'>
            <div class="left">Awesome WRY Window</div>
            <div class="right">
              <div class="titlebar-button" id="minimize">
                <img src="https://api.iconify.design/codicon:chrome-minimize.svg" />
              </div>
              <div class="titlebar-button" id="maximize">
                <img src="https://api.iconify.design/codicon:chrome-maximize.svg" />
              </div>
              <div class="titlebar-button" id="close">
                <img src="https://api.iconify.design/codicon:close.svg" />
              </div>
            </div>
          </div>
          <div>
            WRYYYYYYYYYYYYYYYYYYYYYY!
          </div>
        </body>
      "#;

  let handler = |window: &Window, req: RpcRequest| {
    if req.method == "minimize" {
      window.set_minimized(true);
    }
    if req.method == "maximize" {
      if window.is_maximized() {
        window.set_maximized(false);
      } else {
        window.set_maximized(true);
      }
    }
    /* TODO handle close
    if req.method == "close" {
      proxy.close().unwrap();
    }
    */
    None
  };
  let webview = WebViewBuilder::new(window)
    .unwrap()
    .with_url(url)?
    .with_rpc_handler(handler)
    .with_initialization_script(
      r#"
      (function () {
        window.addEventListener('DOMContentLoaded', (event) => {
          document.getElementById('minimize').addEventListener('click', () => rpc.notify('minimize'));
          document.getElementById('maximize').addEventListener('click', () => rpc.notify('maximize'));
          document.getElementById('close').addEventListener('click', () => rpc.notify('close'));

          const style = document.createElement('style');
          style.textContent = `
            * {
              padding: 0;
              margin: 0;
              box-sizing: border-box;
            }
            .titlebar {
              height: 30px;
              background: #1F1F1F;
              color: white;
              user-select: none;
              display: flex;
              justify-content: space-between;
              align-items: center;
            }
            .titlebar-button {
              display: inline-flex;
              justify-content: center;
              align-items: center;
              width: 30px;
              height: 30px;
            }
            .titlebar-button:hover {
              background: #3b3b3b;
            }
            .titlebar-button:nth-child(3):hover {
              background: #da3d3d;
            }
            .titlebar-button img {
              filter: invert(100%);
            }
          `;
          document.head.append(style);
        });
      })();
      "#,
    )
    .build()?;

  event_loop.run(move |event, _, control_flow| {
    *control_flow = ControlFlow::Poll;

    match event {
      Event::WindowEvent {
        event: WindowEvent::CloseRequested,
        ..
      } => *control_flow = ControlFlow::Exit,
      _ => {
        let _ = webview.resize();
      }
    }
  });
}
