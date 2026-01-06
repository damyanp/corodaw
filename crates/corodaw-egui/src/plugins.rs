use std::rc::{Rc, Weak};

use engine::plugins::{ClapPlugin, ClapPluginManager};
use futures_channel::mpsc::unbounded;
use plugin_ui_host::PluginUiHost;
use smol::LocalExecutor;

use crate::Spawner;

pub struct EguiClapPluginManager {
    pub inner: Rc<ClapPluginManager>,
    ui_host: Rc<PluginUiHost<Spawner>>,
}

impl EguiClapPluginManager {
    pub fn new(executor: Rc<LocalExecutor<'static>>) -> Rc<Self> {
        let (gui_sender, gui_receiver) = unbounded();

        let inner = ClapPluginManager::new(gui_sender);
        Self::spawn_message_handler(&executor, Rc::downgrade(&inner));

        

        Rc::new(Self {
            inner,
            ui_host: PluginUiHost::new(Spawner(executor.clone()), gui_receiver),
        })
    }

    fn spawn_message_handler(executor: &LocalExecutor, manager: Weak<ClapPluginManager>) {
        executor
            .spawn(async move {
                ClapPluginManager::message_handler(manager).await;
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
