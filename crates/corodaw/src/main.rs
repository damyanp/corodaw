use std::{cell::RefCell, rc::Rc};

use gpui::*;
use gpui_component::{
    button::*,
    select::{SearchableVec, Select, SelectItem, SelectState},
    slider::{Slider, SliderEvent, SliderState},
    *,
};

use crate::{
    audio::Audio,
    audio_graph::{
        AudioGraph, NodeId, audio_graph, clap_adapter::get_audio_graph_node_desc_for_clap_plugin,
    },
    builtin::GainControl,
    plugins::{
        ClapPlugin,
        discovery::{FoundPlugin, get_plugins},
    },
};

mod audio;
mod audio_graph;
mod builtin;
mod plugins;

struct Module {
    name: String,
    plugin: Rc<ClapPlugin>,
    gain_slider: Entity<SliderState>,
    _gain: Entity<GainControl>,
    _plugin_id: NodeId,
    _gain_id: NodeId,
}

impl Module {
    pub async fn new(
        name: String,
        mut plugin: RefCell<FoundPlugin>,
        audio_graph: Rc<RefCell<AudioGraph>>,
        cx: &mut AsyncApp,
    ) -> Self {
        let initial_gain = 1.0;
        let gain_slider = cx
            .new(|_| {
                SliderState::new()
                    .default_value(initial_gain)
                    .min(0.0)
                    .max(1.0)
                    .step(0.01)
            })
            .unwrap();
        let gain = cx.new(|_| GainControl::default()).unwrap();

        let plugin = RefCell::get_mut(&mut plugin);

        let plugin = ClapPlugin::new(plugin, cx).await;

        let mut audio_graph = audio_graph.borrow_mut();
        let plugin_id = audio_graph.add_node(get_audio_graph_node_desc_for_clap_plugin(&plugin));

        let gain_id = gain
            .update(cx, |gain, _| {
                audio_graph.add_node(gain.get_node_desc(initial_gain))
            })
            .unwrap();

        audio_graph.connect(plugin_id, 0, gain_id, 0);
        audio_graph.set_output_node(gain_id, true);

        let gain_for_subscription = gain.clone();
        cx.subscribe(&gain_slider, move |_, event, cx| match event {
            SliderEvent::Change(slider_value) => {
                gain_for_subscription.update(cx, |gain, _| gain.set_gain(slider_value.start()))
            }
        })
        .unwrap()
        .detach();

        Self {
            name,
            plugin,
            _gain: gain,
            gain_slider,
            _plugin_id: plugin_id,
            _gain_id: gain_id,
        }
    }

    fn on_show(&mut self, _e: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.plugin.show_gui(window, cx);
    }
}

impl Render for Module {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let show_disabled = self.plugin.has_gui();

        div()
            .border_1()
            .border_color(cx.theme().border)
            .p_5()
            .child(
                h_flex()
                    .gap_2()
                    .child(self.name.clone())
                    .child(Slider::new(&self.gain_slider).min_w_128())
                    .child(
                        Button::new("show")
                            .label("Show")
                            .disabled(show_disabled)
                            .on_click(cx.listener(Self::on_show)),
                    ),
            )
    }
}

#[derive(Clone)]
struct SelectablePlugin(RefCell<FoundPlugin>);

impl SelectablePlugin {
    fn new(p: &RefCell<FoundPlugin>) -> Self {
        SelectablePlugin(p.clone())
    }
}

impl SelectItem for SelectablePlugin {
    type Value = RefCell<FoundPlugin>;

    fn title(&self) -> SharedString {
        self.0.borrow().name.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.0
    }
}

pub struct Corodaw {
    _plugins: Vec<RefCell<FoundPlugin>>,
    plugin_selector: Entity<SelectState<SearchableVec<SelectablePlugin>>>,
    modules: Vec<Entity<Module>>,
    counter: u32,
    audio_graph: Rc<RefCell<AudioGraph>>,
    _audio: Audio,
}

impl Corodaw {
    fn new(
        plugins: Vec<RefCell<FoundPlugin>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (audio_graph, audio_graph_worker) = audio_graph();
        let audio = Audio::new(audio_graph_worker).unwrap();

        let searchable_plugins = SearchableVec::new(
            plugins
                .iter()
                .map(SelectablePlugin::new)
                .collect::<Vec<_>>(),
        );

        let plugin_selector = cx.new(|cx| SelectState::new(searchable_plugins, None, window, cx));

        Self {
            _plugins: plugins,
            plugin_selector,
            modules: Vec::default(), //vec![cx.new(|cx| Module::new(cx, "Master".to_owned()))],
            counter: 0,
            audio_graph: Rc::new(RefCell::new(audio_graph)),
            _audio: audio,
        }
    }

    fn on_click(&mut self, _: &ClickEvent, _: &mut Window, cx: &mut Context<Self>) {
        let plugin = self
            .plugin_selector
            .read(cx)
            .selected_value()
            .expect("The Add button should only be enabled if a plugin is selected")
            .clone();

        let name = format!("Module {}: {}", self.counter, plugin.borrow().name);
        self.counter += 1;

        let audio_graph = self.audio_graph.clone();

        cx.spawn(async move |e, cx| {
            let module = Module::new(name, plugin, audio_graph, cx).await;

            e.update(cx, |corodaw, cx| {
                let module = cx.new(|_| module);
                corodaw.modules.push(module);
            })
            .unwrap();
        })
        .detach();
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
        let plugin = plugin.borrow();
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
    });

    println!("[main] exit");
}
