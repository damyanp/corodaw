use std::rc::{Rc, Weak};

use engine::plugins::{ClapPlugin, ClapPluginManager, GuiMessage};
use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, unbounded};
use plugin_ui_host::PluginUiHost;
use smol::LocalExecutor;

pub struct EguiClapPluginManager {
    pub inner: Rc<ClapPluginManager>,
    ui_host: Rc<PluginUiHost>,
}

impl EguiClapPluginManager {
    pub fn new(executor: &LocalExecutor) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);
        Self::spawn_message_handler(executor, Rc::downgrade(&inner));

        let manager = Rc::new(Self {
            inner,
            ui_host: PluginUiHost::new(),
        });
        Self::spawn_gui_message_handler(executor, Rc::downgrade(&manager), gui_receiver);
        Self::spawn_ui_host_message_handler(executor, manager.ui_host.clone());

        manager
    }

    fn spawn_message_handler(executor: &LocalExecutor, manager: Weak<ClapPluginManager>) {
        executor
            .spawn(async move {
                ClapPluginManager::message_handler(manager).await;
            })
            .detach();
    }

    fn spawn_gui_message_handler(
        executor: &LocalExecutor,
        manager: Weak<Self>,
        mut receiver: UnboundedReceiver<GuiMessage>,
    ) {
        executor
            .spawn(async move {
                println!("[gui_message_handler] start");
                while let Some(GuiMessage { plugin_id, payload }) = receiver.next().await {
                    let Some(manager) = manager.upgrade() else {
                        break;
                    };
                    manager
                        .ui_host
                        .handle_gui_message(GuiMessage { plugin_id, payload });
                }
                println!("[gui_message_handler] end");
            })
            .detach();
    }

    fn spawn_ui_host_message_handler(
        executor: &LocalExecutor<'_>,
        plugin_ui_host: Rc<PluginUiHost>,
    ) {
        executor
            .spawn(async move {
                println!("[ui_host_message_handler] start");
                plugin_ui_host.message_handler().await;
                println!("[ui_host_message_handler] end");
            })
            .detach();
    }

    pub async fn show_plugin_gui(&self, clap_plugin: Rc<ClapPlugin>) {
        println!("show gui");
        self.ui_host.show_gui(&clap_plugin).await;
    }

    pub fn has_plugin_gui(&self, plugin: &ClapPlugin) -> bool {
        self.ui_host.has_gui(plugin)
    }
}
