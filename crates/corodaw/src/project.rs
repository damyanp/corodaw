#![allow(unused)]
use clack_host::host::{self, HostHandlers, HostInfo};
use futures::StreamExt;
use futures_channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, AsyncApp};
use std::{
    cell::RefCell,
    ffi::{CStr, CString},
    rc::{Rc, Weak},
    time::Duration,
};

use crate::plugins::FoundPlugin;

struct Project {
    channels: Vec<Channel>,
}

struct Channel {
    generator: PluginInstance,
}

pub struct PluginInstance {
    plugin: clack_host::plugin::PluginInstance<Self>,
}

impl PluginInstance {
    pub fn new(plugin: &mut FoundPlugin, app: &App) -> Rc<RefCell<Self>> {
        let (sender, mut receiver) = unbounded();

        let bundle = plugin.load_bundle();
        bundle
            .get_plugin_factory()
            .expect("Only bundles with plugin factories supported");

        let id = plugin.id.clone();

        let shared = SharedHandler { channel: sender };
        let main_thread = MainThreadHandler;
        let plugin_id = CString::new(id.as_str()).unwrap();
        let host =
            HostInfo::new("corodaw", "damyanp", "https://github.com/damyanp", "0.0.1").unwrap();

        let plugin = clack_host::plugin::PluginInstance::new(
            move |_| shared,
            move |_| main_thread,
            &bundle,
            plugin_id.as_c_str(),
            &host,
        )
        .unwrap();

        let p = Rc::new(RefCell::new(Self { plugin }));

        let weak_p = Rc::downgrade(&p);

        app.spawn(async move |app| {
            println!("[{}] spawn message receiver", id);
            PluginInstance::handle_messages(weak_p, receiver, app.clone()).await;
            println!("[{}] end message receiver", id);
        })
        .detach();

        p
    }

    async fn handle_messages(
        mut this: Weak<RefCell<PluginInstance>>,
        mut receiver: UnboundedReceiver<Message>,
        app: AsyncApp,
    ) {
        while let Some(msg) = receiver.next().await {
            match msg {
                Message::RunOnMainThread => {
                    if let Some(p) = Weak::upgrade(&this) {
                        p.borrow_mut().plugin.call_on_main_thread_callback();
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
enum Message {
    RunOnMainThread,
}

impl HostHandlers for PluginInstance {
    type Shared<'a> = SharedHandler;
    type MainThread<'a> = MainThreadHandler;
    type AudioProcessor<'a> = AudioProcessorHandler;
}

pub struct SharedHandler {
    channel: UnboundedSender<Message>,
}

unsafe impl Send for SharedHandler {}

impl<'a> host::SharedHandler<'a> for SharedHandler {
    fn request_restart(&self) {
        todo!()
    }

    fn request_process(&self) {
        todo!()
    }

    fn request_callback(&self) {
        todo!()
    }
}

pub struct MainThreadHandler;
impl<'a> host::MainThreadHandler<'a> for MainThreadHandler {
    fn initialized(&mut self, instance: clack_host::prelude::InitializedPluginHandle<'a>) {}
}

pub struct AudioProcessorHandler;
impl<'a> host::AudioProcessorHandler<'a> for AudioProcessorHandler {}
