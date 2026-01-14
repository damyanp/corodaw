use std::{
    cell::RefCell,
    collections::HashMap,
    hash::Hash,
    pin::Pin,
    rc::Rc,
    sync::{Arc, Weak},
};

use crate::plugins::{ClapPlugin, ClapPluginId};
use clack_extensions::gui::{GuiApiType, GuiConfiguration, GuiSize, PluginGui, Window};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::LibraryLoader::GetModuleHandleA,
        UI::WindowsAndMessaging::{
            AdjustWindowRect, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExA,
            DefWindowProcA, DispatchMessageW, GWL_STYLE, GWLP_USERDATA, GetClientRect,
            GetWindowLongPtrA, IDC_ARROW, LoadCursorW, MSG, PM_REMOVE, PeekMessageW,
            RegisterClassA, SWP_ASYNCWINDOWPOS, SWP_NOMOVE, SetWindowLongPtrA, SetWindowPos,
            TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE, WM_DESTROY, WM_SIZE, WNDCLASSA,
            WS_OVERLAPPEDWINDOW, WS_SIZEBOX, WS_THICKFRAME, WS_VISIBLE,
        },
    },
    core::{Error, PCSTR, s},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WindowHandle(HWND);

impl Hash for WindowHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        state.write_usize(self.0.0 as usize);
    }
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

#[derive(Debug)]
pub struct GuiHandle(Weak<WindowHandle>);

impl Hash for GuiHandle {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.as_ptr().hash(state);
    }
}

impl PartialEq for GuiHandle {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_ptr() == other.0.as_ptr()
    }
}

impl Eq for GuiHandle {}

impl GuiHandle {
    pub fn is_visible(&self) -> bool {
        self.0.strong_count() > 0
    }
}

pub struct PluginUiHost {
    plugin_to_window: RefCell<HashMap<ClapPluginId, Arc<WindowHandle>>>,
    window_to_plugin: RefCell<HashMap<WindowHandle, Rc<ClapPlugin>>>,

    wndclass_name: PCSTR,
}

impl PluginUiHost {
    pub fn new() -> PluginUiHost {
        Self {
            plugin_to_window: RefCell::default(),
            window_to_plugin: RefCell::default(),
            wndclass_name: Self::register_window_class(),
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

    pub fn run_message_handlers(&self) {
        self.pump_windows_message_loop();
    }

    fn handle_resized(&self, hwnd: WindowHandle, size: GuiSize) {
        if let Some(clap_plugin) = self.window_to_plugin.borrow().get(&hwnd) {
            let mut plugin = clap_plugin.plugin.borrow_mut();
            let plugin_gui: PluginGui =
                plugin.access_shared_handler(|h| h.extensions.read().unwrap().plugin_gui.unwrap());
            let mut handle = plugin.plugin_handle();
            if plugin_gui.can_resize(&mut handle) {
                plugin_gui.set_size(&mut handle, size).unwrap();
            }
        }
    }

    fn handle_destroyed(&self, hwnd: WindowHandle) {
        if let Some(clap_plugin) = self.window_to_plugin.borrow_mut().remove(&hwnd) {
            self.plugin_to_window
                .borrow_mut()
                .remove(&clap_plugin.get_id());

            let mut plugin = clap_plugin.plugin.borrow_mut();
            let plugin_gui: PluginGui =
                plugin.access_shared_handler(|h| h.extensions.read().unwrap().plugin_gui.unwrap());
            let mut handle = plugin.plugin_handle();
            plugin_gui.destroy(&mut handle);
        }
    }

    pub fn resize_hints_changed(&self, clap_plugin_id: ClapPluginId) {
        let hwnd = self.plugin_to_window.borrow().get(&clap_plugin_id).cloned();
        let clap_plugin = hwnd
            .as_ref()
            .and_then(|wnd| self.window_to_plugin.borrow().get(wnd).cloned());
        if let Some(hwnd) = hwnd
            && let Some(clap_plugin) = clap_plugin
        {
            let gui = clap_plugin
                .plugin
                .borrow()
                .access_shared_handler(|h| h.extensions.read().unwrap().plugin_gui.unwrap());

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

    pub fn request_resize(&self, clap_plugin_id: ClapPluginId, gui_size: GuiSize) {
        let hwnd = self.plugin_to_window.borrow().get(&clap_plugin_id).cloned();
        if let Some(hwnd) = hwnd {
            set_window_client_area(hwnd.0, gui_size);
        }
    }

    pub async fn show_gui(self: &Pin<Box<Self>>, clap_plugin: &Rc<ClapPlugin>) -> GuiHandle {
        let plugin_id = clap_plugin.get_id();
        if self.plugin_to_window.borrow().contains_key(&plugin_id) {
            todo!("bring the window to the front or something");
        }

        let plugin_gui = clap_plugin
            .plugin
            .borrow()
            .access_shared_handler(|h| h.extensions.read().unwrap().plugin_gui.unwrap());

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

        let hwnd = self.create_window(initial_size, can_resize).unwrap();
        println!("got window handle: {:?}", hwnd);

        set_window_client_area(hwnd, initial_size);

        let window = Window::from_win32_hwnd(hwnd.0);

        let mut plugin = clap_plugin.plugin.borrow_mut();
        let mut plugin_handle = plugin.plugin_handle();

        unsafe {
            plugin_gui.set_parent(&mut plugin_handle, window).unwrap();
        }

        plugin_gui.show(&mut plugin_handle).unwrap();

        let window_handle = Arc::new(WindowHandle(hwnd));
        let weak_window_handle = Arc::downgrade(&window_handle);

        self.plugin_to_window
            .borrow_mut()
            .insert(plugin_id, window_handle);
        self.window_to_plugin
            .borrow_mut()
            .insert(WindowHandle(hwnd), clap_plugin.clone());

        GuiHandle(weak_window_handle)
    }

    fn pump_windows_message_loop(&self) {
        unsafe {
            loop {
                let mut msg = MSG::default();

                // TODO: something needs to block this loop!
                let r = PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE);
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

    fn create_window(
        self: &Pin<Box<Self>>,
        initial_size: GuiSize,
        can_resize: bool,
    ) -> windows::core::Result<HWND> {
        unsafe {
            let mut style = WS_OVERLAPPEDWINDOW | WS_VISIBLE;
            if !can_resize {
                style &= !WS_THICKFRAME;
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

            let self_pointer: *const PluginUiHost = self.as_ref().get_ref() as *const Self;

            SetWindowLongPtrA(hwnd, GWLP_USERDATA, self_pointer as isize);

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

        let this = (ptr as *const PluginUiHost).as_ref().unwrap();

        match message {
            WM_SIZE => {
                let width = lparam.0 as u32 & 0xFFFF;
                let height = (lparam.0 as u32 >> 16) & 0xFFFF;
                let size = GuiSize { width, height };

                this.handle_resized(WindowHandle(window), size);

                LRESULT(0)
            }
            WM_DESTROY => {
                this.handle_destroyed(WindowHandle(window));
                LRESULT(0)
            }
            _ => DefWindowProcA(window, message, wparam, lparam),
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
