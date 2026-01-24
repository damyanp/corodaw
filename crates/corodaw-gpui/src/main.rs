use std::time::Duration;

use bevy_app::Update;
use bevy_ecs::{
    query::{Added, Changed, Or},
    system::{NonSend, Query},
};
use engine::plugins::discovery::{FoundPlugin, get_plugins};
use project::*;

use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    *,
};

use crate::module::Module;

mod module;

#[derive(Clone)]
struct SelectablePlugin(FoundPlugin);

impl SelectablePlugin {
    fn new(p: FoundPlugin) -> Self {
        Self(p)
    }
}

impl SelectItem for SelectablePlugin {
    type Value = FoundPlugin;

    fn title(&self) -> SharedString {
        SharedString::new(self.0.name.as_str())
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub struct CorodawApp(bevy_app::App);
impl Global for CorodawApp {}

pub struct Corodaw {
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Entity<Module>>,
}

fn update_channels(
    gpui: NonSend<GpuiContext>,
    new_channels: Query<(bevy_ecs::entity::Entity, &ChannelState), Added<ChannelState>>,
) {
    #[derive(Debug)]
    struct NewChannelInfo {
        entity: bevy_ecs::entity::Entity,
        state: ChannelState,
    }

    let new_channels: Vec<_> = new_channels
        .iter()
        .map(|(entity, state)| NewChannelInfo {
            entity,
            state: (*state).clone(),
        })
        .collect();

    gpui.spawn_update_corodaw(move |corodaw, cx| {
        for channel in &new_channels {
            corodaw
                .modules
                .push(cx.new(|cx| Module::new(channel.entity, channel.state.gain_value, cx)));
        }
    })
    .detach();
}

#[allow(clippy::type_complexity)]
fn data_changed(
    gpui: NonSend<GpuiContext>,
    _: Query<bevy_ecs::entity::Entity, Or<(Changed<ChannelAudioView>, Changed<ChannelData>)>>,
) {
    // we don't care about the actual items returned, just whether _something_ has changed.
    gpui.spawn(async |app| {
        app.refresh().unwrap();
    })
    .detach();
}

struct GpuiContext {
    app: AsyncApp,
    corodaw: Entity<Corodaw>,
}

impl GpuiContext {
    pub fn spawn<AsyncFn, R>(&self, f: AsyncFn) -> Task<R>
    where
        AsyncFn: AsyncFnOnce(&mut AsyncApp) -> R + 'static,
        R: 'static,
    {
        self.app.spawn(f)
    }

    fn spawn_update_corodaw<R: 'static>(
        &self,
        update: impl FnOnce(&mut Corodaw, &mut Context<'_, Corodaw>) -> R + 'static,
    ) -> Task<Result<R>> {
        let corodaw = self.corodaw.clone();
        self.spawn(async move |app| app.update_entity(&corodaw, update))
    }
}

impl Corodaw {
    fn new(plugins: Vec<FoundPlugin>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut app = project::make_app();

        app.insert_non_send_resource(GpuiContext {
            app: cx.to_async(),
            corodaw: cx.entity(),
        })
        .add_systems(Update, (update_channels, data_changed));

        cx.set_global(CorodawApp(app));

        let searchable_plugins = SearchableVec::new(
            plugins
                .into_iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            plugin_selector,
            modules: Default::default(),
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let _ = self
            .plugin_selector
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected")
            .clone();
    }
}

impl Render for Corodaw {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_2()
            .size_full()
            .items_start()
            .justify_start()
            .p_10()
            .child(
                div()
                    .h_flex()
                    .gap_2()
                    .w_full()
                    .child(
                        Button::new("ok")
                            .primary()
                            .label("Add Module")
                            .on_click(cx.listener(Self::on_click))
                            .disabled(self.plugin_selector.read(cx).selected_value().is_none()),
                    )
                    .child(Select::new(&self.plugin_selector)),
            )
            .children(self.modules.iter().map(|m| div().w_full().child(m.clone())))
    }
}

fn main() {
    let app = Application::new();

    let plugins = get_plugins();

    println!("Found {} plugins", plugins.len());
    for plugin in &plugins {
        println!("{}: {} ({})", plugin.id, plugin.name, plugin.path.display());
    }

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|cx| Corodaw::new(plugins, window, cx));
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();

        cx.spawn(async move |cx| {
            loop {
                cx.update_global(|c: &mut CorodawApp, _| {
                    c.0.update();
                })
                .unwrap();
                Timer::interval(Duration::from_millis(16)).await;
            }
        })
        .detach();
    });

    println!("[main] exit");
}
