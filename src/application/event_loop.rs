// Copyright 2019-2021 Tauri Programme within The Commons Conservancy
// SPDX-License-Identifier: Apache-2.0
// SPDX-License-Identifier: MIT

//! The `EventLoop` struct and assorted supporting types, including `ControlFlow`.
//!
//! If you want to send custom events to the event loop, use [`EventLoop::create_proxy()`][create_proxy]
//! to acquire an [`EventLoopProxy`][event_loop_proxy] and call its [`send_event`][send_event] method.
//!
//! See the root-level documentation for information on how to create and use an event loop to
//! handle events.
//!
//! [create_proxy]: crate::event_loop::EventLoop::create_proxy
//! [event_loop_proxy]: crate::event_loop::EventLoopProxy
//! [send_event]: crate::event_loop::EventLoopProxy::send_event
use std::{
  cell::RefCell,
  collections::HashSet,
  error::Error,
  fmt,
  ops::Deref,
  process,
  rc::Rc,
  sync::mpsc::{channel, Receiver, SendError, Sender},
};

use gdk::{Cursor, CursorType, WindowExt};
use gio::{prelude::*, Cancellable};
use glib::{source::idle_add_local, Continue, MainContext};
use gtk::{prelude::*, ApplicationWindow, Inhibit};
pub use winit::event_loop::{ControlFlow, EventLoopClosed};
use winit::window::CursorIcon;

use super::{
  event::{Event, StartCause, WindowEvent},
  window::{WindowId, WindowRequest},
};

/// Target that associates windows with an `EventLoop`.
///
/// This type exists to allow you to create new windows while Winit executes
/// your callback. `EventLoop` will coerce into this type (`impl<T> Deref for
/// EventLoop<T>`), so functions that take this as a parameter can also take
/// `&EventLoop`.
pub struct EventLoopWindowTarget<T> {
  /// Gtk application
  pub(crate) app: gtk::Application,
  /// Window Ids of the application
  pub(crate) windows: Rc<RefCell<HashSet<WindowId>>>,
  /// Window requests sender
  pub(crate) window_requests_tx: Sender<(WindowId, WindowRequest)>,
  /// Window requests receiver
  pub(crate) window_requests_rx: Receiver<(WindowId, WindowRequest)>,
  _marker: std::marker::PhantomData<T>,
  _unsafe: std::marker::PhantomData<*mut ()>, // Not Send nor Sync
}

/// Provides a way to retrieve events from the system and from the windows that were registered to
/// the events loop.
///
/// An `EventLoop` can be seen more or less as a "context". Calling `EventLoop::new()`
/// initializes everything that will be required to create windows. For example on Linux creating
/// an event loop opens a connection to the X or Wayland server.
///
/// To wake up an `EventLoop` from a another thread, see the `EventLoopProxy` docs.
///
/// Note that the `EventLoop` cannot be shared across threads (due to platform-dependant logic
/// forbidding it), as such it is neither `Send` nor `Sync`. If you need cross-thread access, the
/// `Window` created from this `EventLoop` _can_ be sent to an other thread, and the
/// `EventLoopProxy` allows you to wake up an `EventLoop` from another thread.
pub struct EventLoop<T: 'static> {
  /// Window target.
  window_target: EventLoopWindowTarget<T>,
  /// User event sender for EventLoopProxy
  user_event_tx: Sender<T>,
  /// User event receiver
  user_event_rx: Receiver<T>,
  _unsafe: std::marker::PhantomData<*mut ()>, // Not Send nor Sync
}

impl<T> fmt::Debug for EventLoop<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.pad("EventLoop { .. }")
  }
}

impl<T> fmt::Debug for EventLoopWindowTarget<T> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.pad("EventLoopWindowTarget { .. }")
  }
}

impl EventLoop<()> {
  /// Builds a new event loop with a `()` as the user event type.
  ///
  /// ***For cross-platform compatibility, the `EventLoop` must be created on the main thread.***
  /// Attempting to create the event loop on a different thread will panic. This restriction isn't
  /// strictly necessary on all platforms, but is imposed to eliminate any nasty surprises when
  /// porting to platforms that require it. `EventLoopExt::new_any_thread` functions are exposed
  /// in the relevant `platform` module if the target platform supports creating an event loop on
  /// any thread.
  ///
  /// Usage will result in display backend initialisation, this can be controlled on linux
  /// using an environment variable `WINIT_UNIX_BACKEND`. Legal values are `x11` and `wayland`.
  /// If it is not set, winit will try to connect to a wayland connection, and if it fails will
  /// fallback on x11. If this variable is set with any other value, winit will panic.
  ///
  /// ## Platform-specific
  ///
  /// - **iOS:** Can only be called on the main thread.
  pub fn new() -> EventLoop<()> {
    EventLoop::<()>::with_user_event()
  }
}

impl<T: 'static> EventLoop<T> {
  /// Builds a new event loop.
  ///
  /// All caveats documented in [`EventLoop::new`] apply to this function.
  ///
  /// ## Platform-specific
  ///
  /// - **iOS:** Can only be called on the main thread.
  pub fn with_user_event() -> EventLoop<()> {
    assert_is_main_thread("new_any_thread");
    EventLoop::new_any_thread()
  }

  pub(crate) fn new_any_thread() -> EventLoop<T> {
    EventLoop::new_gtk_any_thread().expect("Failed to initialize any backend!")
  }

  fn new_gtk_any_thread() -> Result<EventLoop<T>, Box<dyn Error>> {
    let app = gtk::Application::new(Some("org.tauri.wry"), gio::ApplicationFlags::empty())?;
    let cancellable: Option<&Cancellable> = None;
    app.register(cancellable)?;

    // Create event loop window target.
    let (window_requests_tx, window_requests_rx) = channel();
    let window_target = EventLoopWindowTarget {
      app,
      windows: Rc::new(RefCell::new(HashSet::new())),
      window_requests_tx,
      window_requests_rx,
      _marker: std::marker::PhantomData,
      _unsafe: std::marker::PhantomData,
    };

    // Create user event channel
    let (user_event_tx, user_event_rx) = channel();

    // Create event loop itself.
    let event_loop = Self {
      window_target,
      user_event_tx,
      user_event_rx,
      _unsafe: std::marker::PhantomData,
    };

    Ok(event_loop)
  }

  /// Hijacks the calling thread and initializes the winit event loop with the provided
  /// closure. Since the closure is `'static`, it must be a `move` closure if it needs to
  /// access any data from the calling context.
  ///
  /// See the [`ControlFlow`] docs for information on how changes to `&mut ControlFlow` impact the
  /// event loop's behavior.
  ///
  /// Any values not passed to this function will *not* be dropped.
  ///
  /// [`ControlFlow`]: crate::event_loop::ControlFlow
  #[inline]
  pub fn run<F>(self, callback: F) -> !
  where
    F: FnMut(Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
  {
    self.run_return(callback);
    process::exit(0)
  }

  pub(crate) fn run_return<F>(self, mut callback: F)
  where
    F: FnMut(Event<'_, T>, &EventLoopWindowTarget<T>, &mut ControlFlow) + 'static,
  {
    let mut control_flow = ControlFlow::default();
    let window_target = self.window_target;
    let (event_tx, event_rx) = channel::<Event<'_, T>>();

    // Send closed event when a window is removed
    let windows = window_target.windows.clone();
    for id in windows.take() {
      let windows_rc = window_target.windows.clone();
      let tx_clone = event_tx.clone();
      let window = window_target
        .app
        .get_window_by_id(id.0)
        .expect("Window not found in the application!");
      window.connect_delete_event(move |_, _| {
        windows_rc.borrow_mut().remove(&id);
        if let Err(e) = tx_clone.send(Event::WindowEvent {
          window_id: id,
          event: WindowEvent::CloseRequested,
        }) {
          log::warn!("Failed to send window close event to event channel: {}", e);
        }

        Inhibit(false)
      });
    }

    // Send StartCause::Init event
    let tx_clone = event_tx.clone();
    window_target.app.connect_activate(move |_| {
      if let Err(e) = tx_clone.send(Event::NewEvents(StartCause::Init)) {
        log::warn!("Failed to send init event to event channel: {}", e);
      }
    });
    window_target.app.activate();

    let context = MainContext::default();
    context.push_thread_default();
    let keep_running = Rc::new(RefCell::new(true));
    let keep_running_ = keep_running.clone();
    let user_event_rx = self.user_event_rx;
    idle_add_local(move || {
      // User event
      if let Ok(event) = user_event_rx.try_recv() {
        if let Err(e) = event_tx.send(Event::UserEvent(event)) {
          log::warn!("Failed to send user event to event channel: {}", e);
        }
      }

      // Widnow Request
      if let Ok((id, request)) = window_target.window_requests_rx.try_recv() {
        let window = window_target
          .app
          .get_window_by_id(id.0)
          .expect("Failed to send closed window event!");

        match request {
          WindowRequest::Title(title) => window.set_title(&title),
          WindowRequest::Position((x, y)) => window.move_(x, y),
          WindowRequest::Size((w, h)) => window.resize(w, h),
          WindowRequest::MinSize((min_width, min_height)) => window
            .set_geometry_hints::<ApplicationWindow>(
              None,
              Some(&gdk::Geometry {
                min_width,
                min_height,
                max_width: 0,
                max_height: 0,
                base_width: 0,
                base_height: 0,
                width_inc: 0,
                height_inc: 0,
                min_aspect: 0f64,
                max_aspect: 0f64,
                win_gravity: gdk::Gravity::Center,
              }),
              gdk::WindowHints::MIN_SIZE,
            ),
          WindowRequest::MaxSize((max_width, max_height)) => window
            .set_geometry_hints::<ApplicationWindow>(
              None,
              Some(&gdk::Geometry {
                min_width: 0,
                min_height: 0,
                max_width,
                max_height,
                base_width: 0,
                base_height: 0,
                width_inc: 0,
                height_inc: 0,
                min_aspect: 0f64,
                max_aspect: 0f64,
                win_gravity: gdk::Gravity::Center,
              }),
              gdk::WindowHints::MAX_SIZE,
            ),
          WindowRequest::Visible(visible) => {
            if visible {
              window.show();
            } else {
              window.hide();
            }
          }
          WindowRequest::Resizable(resizable) => window.set_resizable(resizable),
          WindowRequest::Minimized(minimized) => {
            if minimized {
              window.iconify();
            } else {
              window.deiconify();
            }
          }
          WindowRequest::Maximized(maximized) => {
            if maximized {
              window.maximize();
            } else {
              window.unmaximize();
            }
          }
          WindowRequest::DragWindow => {
            let display = window.get_display();
            if let Some(cursor) = display
              .get_device_manager()
              .and_then(|device_manager| device_manager.get_client_pointer())
            {
              let (_, x, y) = cursor.get_position();
              window.begin_move_drag(1, x, y, 0);
            }
          }
          WindowRequest::Fullscreen(fullscreen) => match fullscreen {
            Some(_) => window.fullscreen(),
            None => window.unfullscreen(),
          },
          WindowRequest::Decorations(decorations) => window.set_decorated(decorations),
          WindowRequest::AlwaysOnTop(always_on_top) => window.set_keep_above(always_on_top),
          WindowRequest::WindowIcon(window_icon) => {
            if let Some(icon) = window_icon {
              window.set_icon(Some(&icon.inner));
            }
          }
          WindowRequest::UserAttention(request_type) => {
            if request_type.is_some() {
              window.set_urgency_hint(true)
            }
          }
          WindowRequest::SkipTaskbar => window.set_skip_taskbar_hint(true),
          WindowRequest::CursorIcon(cursor) => {
            if let Some(gdk_window) = window.get_window() {
              let display = window.get_display();
              match cursor {
                Some(cr) => gdk_window.set_cursor(
                  Cursor::from_name(
                    &display,
                    match cr {
                      CursorIcon::Crosshair => "crosshair",
                      CursorIcon::Hand => "pointer",
                      CursorIcon::Arrow => "crosshair",
                      CursorIcon::Move => "move",
                      CursorIcon::Text => "text",
                      CursorIcon::Wait => "wait",
                      CursorIcon::Help => "help",
                      CursorIcon::Progress => "progress",
                      CursorIcon::NotAllowed => "not-allowed",
                      CursorIcon::ContextMenu => "context-menu",
                      CursorIcon::Cell => "cell",
                      CursorIcon::VerticalText => "vertical-text",
                      CursorIcon::Alias => "alias",
                      CursorIcon::Copy => "copy",
                      CursorIcon::NoDrop => "no-drop",
                      CursorIcon::Grab => "grab",
                      CursorIcon::Grabbing => "grabbing",
                      CursorIcon::AllScroll => "all-scroll",
                      CursorIcon::ZoomIn => "zoom-in",
                      CursorIcon::ZoomOut => "zoom-out",
                      CursorIcon::EResize => "e-resize",
                      CursorIcon::NResize => "n-resize",
                      CursorIcon::NeResize => "ne-resize",
                      CursorIcon::NwResize => "nw-resize",
                      CursorIcon::SResize => "s-resize",
                      CursorIcon::SeResize => "se-resize",
                      CursorIcon::SwResize => "sw-resize",
                      CursorIcon::WResize => "w-resize",
                      CursorIcon::EwResize => "ew-resize",
                      CursorIcon::NsResize => "ns-resize",
                      CursorIcon::NeswResize => "nesw-resize",
                      CursorIcon::NwseResize => "nwse-resize",
                      CursorIcon::ColResize => "col-resize",
                      CursorIcon::RowResize => "row-resize",
                      CursorIcon::Default => "default",
                    },
                  )
                  .as_ref(),
                ),
                None => gdk_window.set_cursor(Some(&Cursor::new_for_display(
                  &display,
                  CursorType::BlankCursor,
                ))),
              }
            };
          }
        }
      }

      // Event control flow
      match control_flow {
        ControlFlow::Exit => {
          keep_running_.replace(false);
          Continue(false)
        }
        // TODO better control flow handling
        _ => {
          if let Ok(event) = event_rx.try_recv() {
            callback(event, &window_target, &mut control_flow);
          } else {
            callback(Event::MainEventsCleared, &window_target, &mut control_flow);
          }
          Continue(true)
        }
      }
    });
    context.pop_thread_default();

    while *keep_running.borrow() {
      gtk::main_iteration();
    }
  }

  #[inline]
  pub fn window_target(&self) -> &EventLoopWindowTarget<T> {
    &self.window_target
  }

  /// Creates an `EventLoopProxy` that can be used to dispatch user events to the main event loop.
  pub fn create_proxy(&self) -> EventLoopProxy<T> {
    EventLoopProxy {
      user_event_tx: self.user_event_tx.clone(),
    }
  }
}

impl<T> Deref for EventLoop<T> {
  type Target = EventLoopWindowTarget<T>;
  fn deref(&self) -> &EventLoopWindowTarget<T> {
    self.window_target()
  }
}

/// Used to send custom events to `EventLoop`.
#[derive(Debug, Clone)]
pub struct EventLoopProxy<T: 'static> {
  user_event_tx: Sender<T>,
}

impl<T: 'static> EventLoopProxy<T> {
  /// Send an event to the `EventLoop` from which this proxy was created. This emits a
  /// `UserEvent(event)` event in the event loop, where `event` is the value passed to this
  /// function.
  ///
  /// Returns an `Err` if the associated `EventLoop` no longer exists.
  pub fn send_event(&self, event: T) -> Result<(), EventLoopClosed<T>> {
    self
      .user_event_tx
      .send(event)
      .map_err(|SendError(error)| EventLoopClosed(error))
  }
}

fn assert_is_main_thread(suggested_method: &str) {
  if !is_main_thread() {
    panic!(
      "Initializing the event loop outside of the main thread is a significant \
             cross-platform compatibility hazard. If you really, absolutely need to create an \
             EventLoop on a different thread, please use the `EventLoopExtUnix::{}` function.",
      suggested_method
    );
  }
}

#[cfg(target_os = "linux")]
fn is_main_thread() -> bool {
  use libc::{c_long, getpid, syscall, SYS_gettid};

  unsafe { syscall(SYS_gettid) == getpid() as c_long }
}

#[cfg(any(target_os = "dragonfly", target_os = "freebsd", target_os = "openbsd"))]
fn is_main_thread() -> bool {
  use libc::pthread_main_np;

  unsafe { pthread_main_np() == 1 }
}

#[cfg(target_os = "netbsd")]
fn is_main_thread() -> bool {
  std::thread::current().name() == Some("main")
}
