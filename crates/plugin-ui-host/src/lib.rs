use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    hash::Hash,
    rc::Rc,
    sync::{
        Arc, OnceLock,
        mpsc::{Receiver, Sender, channel},
    },
    thread::JoinHandle,
};

use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui, Window};
use engine::plugins::{ClapPlugin, ClapPluginId, GuiMessage, GuiMessagePayload};
use futures::{
    SinkExt, StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded},
};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::{LibraryLoader::GetModuleHandleA, Threading::GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            AdjustWindowRect, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExA,
            DefWindowProcA, DispatchMessageW, GWL_STYLE, GWLP_USERDATA, GetClientRect, GetMessageW,
            GetWindowLongPtrA, GetWindowRect, IDC_ARROW, LoadCursorW, MSG, PostThreadMessageA,
            RegisterClassA, SET_WINDOW_POS_FLAGS, SWP_ASYNCWINDOWPOS, SWP_NOMOVE,
            SetWindowLongPtrA, SetWindowPos, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
            WM_DESTROY, WM_SIZE, WM_USER, WNDCLASSA, WS_OVERLAPPEDWINDOW, WS_SIZEBOX,
            WS_THICKFRAME, WS_VISIBLE,
        },
    },
    core::{Error, PCSTR, s},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowHandle(HWND);

impl Hash for WindowHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(unsafe { std::mem::transmute(self.0.0) });
    }
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

pub struct PluginUiHost {
    thread: JoinHandle<()>,
    thread_id: u32,

    msg_sender: Sender<Message>,
    window_msg_receiver: Cell<Option<UnboundedReceiver<WindowMessage>>>,

    plugin_to_window: RefCell<HashMap<ClapPluginId, WindowHandle>>,
    window_to_plugin: RefCell<HashMap<WindowHandle, Rc<ClapPlugin>>>,
}

impl PluginUiHost {
    pub fn new() -> Rc<PluginUiHost> {
        let thread_id = Arc::new(OnceLock::new());
        let (msg_sender, msg_receiver) = channel();
        let (window_msg_sender, window_msg_receiver) = unbounded();

        let thread_id_clone = thread_id.clone();
        let thread = std::thread::spawn(move || {
            thread_id_clone
                .set(unsafe { GetCurrentThreadId() })
                .unwrap();

            PluginUiHostThread::new(msg_receiver, window_msg_sender).run_message_loop();
        });

        Rc::new(Self {
            thread,
            thread_id: *thread_id.wait(),
            msg_sender,
            window_msg_receiver: Cell::new(Some(window_msg_receiver)),
            plugin_to_window: RefCell::default(),
            window_to_plugin: RefCell::default(),
        })
    }

    pub async fn message_handler(self: &Rc<PluginUiHost>) {
        let this = self.clone();
        let mut receiver = self.window_msg_receiver.take().unwrap();

        while let Some(msg) = receiver.next().await {
            match msg {
                WindowMessage::Resized { hwnd, size } => {
                    if let Some(clap_plugin) = this.window_to_plugin.borrow().get(&hwnd) {
                        let mut plugin = clap_plugin.plugin.borrow_mut();
                        let mut handle = plugin.plugin_handle();
                        let plugin_gui: PluginGui = handle.get_extension().unwrap();
                        if plugin_gui.can_resize(&mut handle) {
                            plugin_gui.set_size(&mut handle, size).unwrap();
                        }
                    }
                }
                WindowMessage::Destroyed { hwnd } => {
                    if let Some(plugin) = self.window_to_plugin.borrow_mut().remove(&hwnd) {
                        self.plugin_to_window.borrow_mut().remove(&plugin.get_id());
                    }
                }
            }
        }
    }

    pub fn rundown(self) {
        let _ = self.thread.join();
    }

    pub fn has_gui(&self, clap_plugin: &Rc<ClapPlugin>) -> bool {
        self.plugin_to_window
            .borrow()
            .contains_key(&clap_plugin.get_id())
    }

    pub async fn show_gui(&self, clap_plugin: &Rc<ClapPlugin>) {
        let plugin_id = clap_plugin.get_id();
        if self.plugin_to_window.borrow().contains_key(&plugin_id) {
            todo!("bring the window to the front or something");
        }

        let plugin_gui: PluginGui = clap_plugin
            .plugin
            .borrow_mut()
            .plugin_handle()
            .get_extension()
            .unwrap();

        let config = GuiConfiguration {
            api_type: GuiApiType::default_for_current_platform()
                .expect("This platform supports UI"),
            is_floating: false,
        };

        plugin_gui
            .create(&mut clap_plugin.plugin.borrow_mut().plugin_handle(), config)
            .unwrap();

        let initial_size = plugin_gui
            .get_size(&mut clap_plugin.plugin.borrow_mut().plugin_handle())
            .unwrap_or(GuiSize {
                width: 800,
                height: 600,
            });

        let size_hints =
            plugin_gui.get_resize_hints(&mut clap_plugin.plugin.borrow_mut().plugin_handle());
        let can_resize = size_hints
            .map(|h| h.can_resize_horizontally && h.can_resize_vertically)
            .unwrap_or(false);

        let (sender, receiver) = futures::channel::oneshot::channel();
        self.send_message(Message::CreatePluginWindow {
            initial_size,
            sender,
            can_resize,
        });
        let hwnd = receiver.await.unwrap();

        set_window_client_area(hwnd.0, initial_size);

        let window = Window::from_win32_hwnd(hwnd.0.0);

        let mut plugin = clap_plugin.plugin.borrow_mut();
        let mut plugin_handle = plugin.plugin_handle();

        unsafe {
            plugin_gui.set_parent(&mut plugin_handle, window).unwrap();
        }

        plugin_gui.show(&mut plugin_handle).unwrap();

        self.plugin_to_window.borrow_mut().insert(plugin_id, hwnd);
        self.window_to_plugin
            .borrow_mut()
            .insert(hwnd, clap_plugin.clone());
    }

    fn send_message(&self, message: Message) {
        self.msg_sender.send(message).unwrap();

        // Post a WM_USER message to wake the thread up so it can receive
        // mesages from the channel.
        unsafe {
            PostThreadMessageA(
                self.thread_id,
                WM_USER,
                WPARAM::default(),
                LPARAM::default(),
            )
            .unwrap()
        };
    }

    pub fn handle_gui_message(&self, message: GuiMessage) {
        match message.payload {
            GuiMessagePayload::ResizeHintsChanged => {
                let hwnd = self
                    .plugin_to_window
                    .borrow()
                    .get(&message.plugin_id)
                    .cloned();
                let clap_plugin = hwnd
                    .as_ref()
                    .and_then(|wnd| self.window_to_plugin.borrow().get(wnd).cloned());
                if let Some(hwnd) = hwnd
                    && let Some(clap_plugin) = clap_plugin
                {
                    let gui: PluginGui = clap_plugin
                        .plugin
                        .borrow_mut()
                        .plugin_handle()
                        .get_extension()
                        .unwrap();

                    let is_resizable = gui
                        .get_resize_hints(&mut clap_plugin.plugin.borrow_mut().plugin_handle())
                        .map(|h| h.can_resize_horizontally && h.can_resize_vertically)
                        .unwrap_or(false);

                    unsafe {
                        let old_style = WINDOW_STYLE(GetWindowLongPtrA(hwnd.0, GWL_STYLE) as u32);
                        let new_style = if is_resizable {
                            old_style | WS_SIZEBOX
                        } else {
                            old_style & !WS_SIZEBOX
                        };
                        if old_style != new_style {
                            SetWindowLongPtrA(hwnd.0, GWL_STYLE, new_style.0 as isize);
                        }
                    }
                }
            }
            GuiMessagePayload::RequestResize(gui_size) => {
                let hwnd = self
                    .plugin_to_window
                    .borrow()
                    .get(&message.plugin_id)
                    .cloned();
                if let Some(hwnd) = hwnd {
                    set_window_client_area(hwnd.0, gui_size);
                }
            }
        }
    }
}

fn set_window_client_area(hwnd: HWND, gui_size: GuiSize) {
    unsafe {
        let mut rect = RECT::default();
        GetClientRect(hwnd, &mut rect).unwrap();

        rect.right = rect.left + gui_size.width as i32;
        rect.bottom = rect.top + gui_size.height as i32;

        let style = GetWindowLongPtrA(hwnd, GWL_STYLE);
        AdjustWindowRect(&mut rect, WINDOW_STYLE(style as u32), false).unwrap();

        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rect.right - rect.left,
            rect.bottom - rect.top,
            SWP_NOMOVE | SWP_ASYNCWINDOWPOS,
        )
        .unwrap();
    }
}

#[derive(Debug)]
enum Message {
    CreatePluginWindow {
        initial_size: GuiSize,
        can_resize: bool,
        sender: futures::channel::oneshot::Sender<WindowHandle>,
    },
}

#[derive(Debug)]
enum WindowMessage {
    Resized { hwnd: WindowHandle, size: GuiSize },
    Destroyed { hwnd: WindowHandle },
}

struct PluginUiHostThread {
    msg_receiver: Receiver<Message>,
    window_msg_sender: Box<UnboundedSender<WindowMessage>>,
    wndclass_name: PCSTR,
}

impl PluginUiHostThread {
    fn new(
        msg_receiver: Receiver<Message>,
        window_msg_sender: UnboundedSender<WindowMessage>,
    ) -> Self {
        let wndclass_name = Self::register_window_class();
        Self {
            msg_receiver,
            window_msg_sender: Box::new(window_msg_sender),
            wndclass_name,
        }
    }

    fn register_window_class() -> PCSTR {
        unsafe {
            let instance = GetModuleHandleA(None).unwrap();
            let window_class = s!("window");

            let wc = WNDCLASSA {
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                hInstance: instance.into(),
                lpszClassName: window_class,

                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(wndproc),
                ..Default::default()
            };

            let atom = RegisterClassA(&wc);
            debug_assert!(atom != 0);
            window_class
        }
    }

    fn run_message_loop(&self) {
        unsafe {
            loop {
                self.handle_messages();

                let mut msg = MSG::default();
                let r = GetMessageW(&mut msg, None, 0, 0);
                if r.0 == 0 {
                    // normal exit
                    break;
                } else if r.0 == -1 {
                    // error - this shouldn't ever happen, so the panic here is to
                    // let us clearly spot cases where it does happen
                    panic!("GetMessageW failed: {:?}", Error::from_thread());
                }

                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn handle_messages(&self) {
        for msg in self.msg_receiver.try_iter() {
            match msg {
                Message::CreatePluginWindow {
                    initial_size,
                    can_resize,
                    sender,
                } => {
                    let window_handle =
                        WindowHandle(self.create_window(initial_size, can_resize).unwrap());
                    sender.send(window_handle);
                }
            }
        }
    }

    fn create_window(
        &self,
        initial_size: GuiSize,
        can_resize: bool,
    ) -> windows::core::Result<HWND> {
        unsafe {
            let mut style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
            if !can_resize {
                style = style & !WS_THICKFRAME;
            }

            let hwnd = CreateWindowExA(
                WINDOW_EX_STYLE::default(),
                self.wndclass_name,
                s!("This is a sample window"),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                initial_size.width as i32,
                initial_size.height as i32,
                None,
                None,
                Some(GetModuleHandleA(None).unwrap().into()),
                None,
            )?;

            SetWindowLongPtrA(
                hwnd,
                GWLP_USERDATA,
                Box::as_ref(&self.window_msg_sender) as *const _ as isize,
            );

            Ok(hwnd)
        }
    }
}

extern "system" fn wndproc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        let ptr = GetWindowLongPtrA(window, GWLP_USERDATA);
        if ptr == 0 {
            return DefWindowProcA(window, message, wparam, lparam);
        }

        let sender = (ptr as *mut UnboundedSender<WindowMessage>)
            .as_mut()
            .unwrap();

        match message {
            WM_SIZE => {
                let width = lparam.0 as u32 & 0xFFFF;
                let height = (lparam.0 as u32 >> 16) & 0xFFFF;
                let size = GuiSize { width, height };

                sender
                    .unbounded_send(WindowMessage::Resized {
                        hwnd: WindowHandle(window),
                        size,
                    })
                    .unwrap();
                LRESULT(0)
            }
            WM_DESTROY => {
                sender
                    .unbounded_send(WindowMessage::Destroyed {
                        hwnd: WindowHandle(window),
                    })
                    .unwrap();
                LRESULT(0)
            }
            _ => DefWindowProcA(window, message, wparam, lparam),
        }
    }
}
